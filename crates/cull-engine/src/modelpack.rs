//! The optional **Maximum** pack (`precision: 'maximum'`): download,
//! verification and installation of its models, and their loading for the
//! analyses that use them.
//!
//! The files (`bundle: "maximum"` in `models/manifest.json`, ~777 MB) go to
//! `<data dir>/models/maximum/`, never the cache dir: clearing the preview
//! cache keeps them. Each file is downloaded into `<file>.part` with HTTP
//! range requests, so a cancelled or interrupted download (app quit,
//! network lost) resumes where it stopped; once complete its SHA-256 is
//! checked against the manifest compiled into the app and it is renamed
//! into place atomically. A file that does not match is downloaded again
//! (3 attempts per URL, then its mirrors), and the pack fails with code
//! `checksum` when none matches.
//!
//! States (`ModelPackState` in the contract): `absent` → `downloading` →
//! `preparing` (each model loaded once and run on reference inputs, see
//! `cull_ml::MaximumModels::check`) → `ready`. `failed` keeps the bytes on
//! disk: code `cancelled` after [`Engine::cancel_model_pack`] and at startup
//! when parts are on disk (the UI offers to resume), `offline`, `diskFull`,
//! `checksum`, or another code with its message. [`Engine::download_model_pack`]
//! resumes from any of them. Every change is sent as a `model-pack` event,
//! at most ~4 per second while downloading.
//!
//! The models are loaded only when an analysis in Maximum precision runs
//! (and released after it), the CLIP L/14 text tower only when a project
//! analysed in Maximum is searched.
//!
//! Debug builds only: `PELLI_MODELS_BASE_URL` replaces the URL of every pack
//! file by `<base>/<file name>` (a local server with range support serving
//! the converted files), without mirrors.

use crate::{Engine, EngineError, Result};
use cull_ml::{MaximumModels, MaximumText, PackFile};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// kv key: files of the pack (name → size, modification time) after a
/// successful preparation. The pack is `ready` at startup when they still
/// match.
const READY_KEY: &str = "model_pack.maximum";
/// Free space kept beyond what the download still needs.
const DISK_MARGIN: u64 = 256 * 1024 * 1024;
/// Minimum time between two `model-pack` events while downloading.
const EMIT_EVERY: Duration = Duration::from_millis(250);
/// Attempts per URL of a file (network errors resume; a checksum mismatch
/// starts the file over).
const ATTEMPTS_PER_URL: usize = 3;
const READ_CHUNK: usize = 256 * 1024;

/// State of the pack (contract `ModelPackState`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ModelPackState {
    Absent,
    #[serde(rename_all = "camelCase")]
    Downloading {
        received_bytes: u64,
        total_bytes: u64,
        bytes_per_second: Option<f64>,
    },
    Preparing {
        progress: f32,
    },
    Ready,
    #[serde(rename_all = "camelCase")]
    Failed {
        code: String,
        message: String,
        received_bytes: u64,
        total_bytes: u64,
    },
}

/// The pack as the UI sees it (contract `ModelPack`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPack {
    pub id: &'static str,
    /// Total download size.
    pub size_bytes: u64,
    /// Space used once installed: the files (the pack runs on the CPU, no
    /// compiled accelerator cache).
    pub disk_bytes: u64,
    /// Free space on the disk of the data folder, `None` when unknown.
    pub free_bytes: Option<u64>,
    pub state: ModelPackState,
}

/// Download and installation of the pack, owned by the [`Engine`].
pub(crate) struct PackManager {
    dir: PathBuf,
    files: Vec<PackFile>,
    state: Mutex<ModelPackState>,
    cancel: AtomicBool,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    last_emit: Mutex<Option<Instant>>,
    /// Loaded models (during analyses in Maximum precision).
    models: Mutex<Option<Arc<MaximumModels>>>,
    /// Loaded text tower (search of projects analysed in Maximum).
    text: Mutex<Option<Arc<MaximumText>>>,
    /// Pause before attempt `n` of a file: `n` × this.
    retry_delay: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    size: u64,
    mtime_ns: u128,
}

fn stamp(path: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    let mtime_ns = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    Some(Stamp { size: m.len(), mtime_ns })
}

fn part_path(dir: &Path, f: &PackFile) -> PathBuf {
    dir.join(format!("{}.part", f.file))
}

/// Why a download stopped.
#[derive(Debug)]
enum Stop {
    Cancelled,
    Failed { code: &'static str, message: String },
}

fn failed(code: &'static str, message: impl Into<String>) -> Stop {
    Stop::Failed { code, message: message.into() }
}

fn io_stop(path: &Path, e: std::io::Error) -> Stop {
    if e.kind() == std::io::ErrorKind::StorageFull {
        failed("diskFull", format!("{}: {e}", path.display()))
    } else {
        failed("internal", format!("{}: {e}", path.display()))
    }
}

impl PackManager {
    pub(crate) fn new(dir: PathBuf) -> PackManager {
        PackManager {
            dir,
            files: cull_ml::maximum_pack_files(),
            state: Mutex::new(ModelPackState::Absent),
            cancel: AtomicBool::new(false),
            worker: Mutex::new(None),
            last_emit: Mutex::new(None),
            models: Mutex::new(None),
            text: Mutex::new(None),
            retry_delay: Duration::from_secs(2),
        }
    }

    fn total(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }

    /// Bytes of the pack already on disk: complete files and parts.
    fn received(&self) -> u64 {
        self.files
            .iter()
            .map(|f| match stamp(&self.dir.join(&f.file)) {
                Some(s) if s.size == f.size => f.size,
                _ => stamp(&part_path(&self.dir, f)).map(|s| s.size.min(f.size)).unwrap_or(0),
            })
            .sum()
    }

    fn complete(&self) -> bool {
        self.files.iter().all(|f| stamp(&self.dir.join(&f.file)).is_some_and(|s| s.size == f.size))
    }

    fn stamps(&self) -> Option<std::collections::BTreeMap<String, Stamp>> {
        self.files.iter().map(|f| Some((f.file.clone(), stamp(&self.dir.join(&f.file))?))).collect()
    }

    fn is_ready(&self) -> bool {
        matches!(*self.state.lock(), ModelPackState::Ready)
    }

    /// Downloading or preparing.
    pub(crate) fn is_busy(&self) -> bool {
        matches!(*self.state.lock(), ModelPackState::Downloading { .. } | ModelPackState::Preparing { .. })
    }
}

/// Free space (bytes) on the disk holding `path` (or its nearest existing
/// parent).
pub fn free_space(path: &Path) -> Option<u64> {
    let mut p = path;
    while !p.exists() {
        p = p.parent()?;
    }
    free_space_of(p)
}

#[cfg(unix)]
// The `statvfs` field types differ between systems (u32 / u64).
#[allow(clippy::useless_conversion)]
fn free_space_of(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: valid NUL-terminated path and a zeroed out-struct.
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut s) };
    (rc == 0).then(|| u64::from(s.f_bavail) * u64::from(s.f_frsize))
}

#[cfg(windows)]
fn free_space_of(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut free: u64 = 0;
    // SAFETY: NUL-terminated wide path; only the first out-pointer is used.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    (ok != 0).then_some(free)
}

#[cfg(not(any(unix, windows)))]
fn free_space_of(_path: &Path) -> Option<u64> {
    None
}

/// URLs to try for `f`, best first.
fn urls(f: &PackFile) -> Vec<String> {
    #[cfg(debug_assertions)]
    if let Ok(base) = std::env::var("PELLI_MODELS_BASE_URL") {
        let base = base.trim().trim_end_matches('/');
        if base.starts_with("http://") || base.starts_with("https://") {
            return vec![format!("{base}/{}", f.file)];
        }
    }
    std::iter::once(f.url.clone()).chain(f.mirrors.iter().cloned()).collect()
}

fn agent() -> ureq::Agent {
    use ureq::tls::{RootCerts, TlsConfig};
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .timeout_per_read(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .user_agent(format!("PelliInstant/{} ({})", env!("CARGO_PKG_VERSION"), crate::pelli::platform()))
        .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
        .proxy(ureq::Proxy::try_from_env())
        .build()
        .into()
}

/// Progress of a download, shared by the files.
struct Progress {
    received: u64,
    total: u64,
    /// Bytes per second, smoothed.
    rate: Option<f64>,
    window: (Instant, u64),
}

impl Progress {
    fn add(&mut self, n: u64) {
        self.received += n;
        let elapsed = self.window.0.elapsed().as_secs_f64();
        if elapsed >= 0.5 {
            let r = (self.received - self.window.1) as f64 / elapsed;
            self.rate = Some(self.rate.map_or(r, |old| 0.6 * r + 0.4 * old));
            self.window = (Instant::now(), self.received);
        }
    }
}

impl Engine {
    /// The pack as the UI sees it.
    pub fn model_pack(&self) -> ModelPack {
        let p = &self.pack;
        let size = p.total();
        ModelPack {
            id: "maximum",
            size_bytes: size,
            disk_bytes: size,
            free_bytes: free_space(&p.dir),
            state: p.state.lock().clone(),
        }
    }

    /// The pack is installed and its models load.
    pub fn maximum_ready(&self) -> bool {
        self.pack.is_ready()
    }

    fn set_pack_state(&self, state: ModelPackState, throttle: bool) {
        let p = &self.pack;
        {
            let mut s = p.state.lock();
            if *s == state {
                return;
            }
            *s = state;
        }
        let now = Instant::now();
        {
            let mut last = p.last_emit.lock();
            if throttle && last.is_some_and(|t| now.duration_since(t) < EMIT_EVERY) {
                return;
            }
            *last = Some(now);
        }
        if let Ok(v) = serde_json::to_value(self.model_pack()) {
            self.events().emit("model-pack", v);
        }
    }

    fn pack_failed(&self, code: &str, message: String) {
        let (received, total) = (self.pack.received(), self.pack.total());
        log::warn!("Maximum pack: {code}: {message}");
        self.set_pack_state(
            ModelPackState::Failed { code: code.into(), message, received_bytes: received, total_bytes: total },
            false,
        );
    }

    /// State of the pack found on disk when the engine opens: `ready` when
    /// its files are those prepared last time, `preparing` (in the
    /// background) when they are complete but were never prepared, a
    /// resumable `failed`/`cancelled` when parts are on disk.
    pub(crate) fn init_model_pack(self: &Arc<Self>) {
        let p = &self.pack;
        let stored: Option<std::collections::BTreeMap<String, Stamp>> =
            self.kv_get(READY_KEY).ok().flatten().and_then(|j| serde_json::from_str(&j).ok());
        let state = if p.complete() {
            if stored.is_some() && stored == p.stamps() {
                ModelPackState::Ready
            } else {
                *p.state.lock() = ModelPackState::Preparing { progress: 0.0 };
                let engine = self.clone();
                self.spawn_pack_worker(move || engine.prepare_model_pack());
                return;
            }
        } else {
            let received = p.received();
            if received > 0 {
                ModelPackState::Failed {
                    code: "cancelled".into(),
                    message: "download paused".into(),
                    received_bytes: received,
                    total_bytes: p.total(),
                }
            } else {
                ModelPackState::Absent
            }
        };
        *p.state.lock() = state;
    }

    fn spawn_pack_worker(&self, f: impl FnOnce() + Send + 'static) {
        let mut worker = self.pack.worker.lock();
        if worker.as_ref().is_some_and(|h| !h.is_finished()) {
            return;
        }
        let job = move || {
            let _awake = crate::system::KeepAwake::new("Pelli Instant: Maximum precision models");
            f()
        };
        match std::thread::Builder::new().name("model-pack".into()).spawn(job) {
            Ok(h) => *worker = Some(h),
            Err(e) => log::error!("cannot start the Maximum pack worker: {e}"),
        }
    }

    /// Start or resume the download, in the background (progress through
    /// `model-pack` events). Errors at once: `diskFull` when the disk cannot
    /// hold what is left to download, `offline` when the first file cannot
    /// be reached at all.
    pub fn download_model_pack(self: &Arc<Self>) -> Result<()> {
        let p = &self.pack;
        match *p.state.lock() {
            ModelPackState::Ready | ModelPackState::Downloading { .. } | ModelPackState::Preparing { .. } => {
                return Ok(())
            }
            _ => {}
        }
        {
            // A cancelled download winding down (one read at most): wait for
            // it, then resume.
            let mut worker = p.worker.lock();
            if let Some(h) = worker.take() {
                if !h.is_finished() && !p.cancel.load(Ordering::SeqCst) {
                    *worker = Some(h);
                    return Ok(());
                }
                let _ = h.join();
            }
        }
        // From here, a cancel (even during the reachability check below)
        // stops this download.
        p.cancel.store(false, Ordering::SeqCst);
        std::fs::create_dir_all(&p.dir).map_err(|e| EngineError::io(&p.dir, e))?;
        let (received, total) = (p.received(), p.total());
        let needed = total.saturating_sub(received) + DISK_MARGIN;
        if let Some(free) = free_space(&p.dir) {
            if free < needed {
                self.pack_failed("diskFull", format!("{needed} bytes needed, {free} free"));
                return Err(EngineError::DiskFull { needed, free });
            }
        }
        if let Some(f) = p.files.iter().find(|f| stamp(&p.dir.join(&f.file)).is_none_or(|s| s.size != f.size)) {
            // One quick request: no network at all is an immediate error.
            let reachable = urls(f).iter().any(|u| {
                agent()
                    .get(u)
                    .header("Range", "bytes=0-0")
                    .header("Accept-Encoding", "identity")
                    .config()
                    .timeout_global(Some(Duration::from_secs(10)))
                    .build()
                    .call()
                    .is_ok()
            });
            if !reachable {
                self.pack_failed("offline", "the download server cannot be reached".into());
                return Err(EngineError::Remote {
                    code: "network".into(),
                    message: "download server unreachable".into(),
                });
            }
        }
        self.set_pack_state(
            ModelPackState::Downloading { received_bytes: received, total_bytes: total, bytes_per_second: None },
            false,
        );
        let engine = self.clone();
        self.spawn_pack_worker(move || match engine.download_files() {
            Ok(()) => engine.prepare_model_pack(),
            Err(Stop::Cancelled) => engine.pack_failed("cancelled", "download paused".into()),
            Err(Stop::Failed { code, message }) => engine.pack_failed(code, message),
        });
        Ok(())
    }

    /// Stop the download, keeping what is on disk (state `failed` with code
    /// `cancelled`).
    pub fn cancel_model_pack(&self) -> ModelPack {
        let p = &self.pack;
        let downloading = matches!(*p.state.lock(), ModelPackState::Downloading { .. });
        p.cancel.store(true, Ordering::SeqCst);
        if downloading {
            self.pack_failed("cancelled", "download paused".into());
        }
        self.model_pack()
    }

    /// Delete the pack (files, parts, loaded models): state `absent`. The
    /// precision setting stays as it is.
    pub fn remove_model_pack(&self) -> Result<ModelPack> {
        let p = &self.pack;
        p.cancel.store(true, Ordering::SeqCst);
        if let Some(h) = p.worker.lock().take() {
            let _ = h.join();
        }
        *p.models.lock() = None;
        *p.text.lock() = None;
        if p.dir.exists() {
            std::fs::remove_dir_all(&p.dir).map_err(|e| EngineError::io(&p.dir, e))?;
        }
        self.kv_delete(READY_KEY)?;
        self.set_pack_state(ModelPackState::Absent, false);
        Ok(self.model_pack())
    }

    /// Download every missing file (worker thread).
    fn download_files(&self) -> std::result::Result<(), Stop> {
        let p = &self.pack;
        let agent = agent();
        let progress = Mutex::new(Progress {
            received: p.received(),
            total: p.total(),
            rate: None,
            window: (Instant::now(), p.received()),
        });
        for f in &p.files {
            let dest = p.dir.join(&f.file);
            if stamp(&dest).is_some_and(|s| s.size == f.size) {
                continue; // complete (its checksum is verified when preparing)
            }
            let mut last = failed("internal", format!("{}: no URL", f.file));
            'urls: for url in urls(f) {
                for attempt in 0..ATTEMPTS_PER_URL {
                    if p.cancel.load(Ordering::SeqCst) {
                        return Err(Stop::Cancelled);
                    }
                    if attempt > 0 {
                        std::thread::sleep(p.retry_delay * attempt as u32);
                    }
                    match self.fetch(&agent, f, &url, &progress) {
                        Ok(()) => break 'urls,
                        Err(Stop::Cancelled) => return Err(Stop::Cancelled),
                        Err(e) => {
                            log::warn!("Maximum pack: {} from {url}: {e:?}", f.file);
                            let next_url = matches!(&e, Stop::Failed { code: "notFound", .. });
                            if matches!(&e, Stop::Failed { code: "diskFull", .. }) {
                                return Err(e);
                            }
                            last = e;
                            if next_url {
                                continue 'urls;
                            }
                        }
                    }
                }
            }
            if stamp(&dest).is_none_or(|s| s.size != f.size) {
                return Err(last);
            }
        }
        Ok(())
    }

    /// One attempt at one file from one URL: resume its part, verify,
    /// rename into place.
    fn fetch(
        &self,
        agent: &ureq::Agent,
        f: &PackFile,
        url: &str,
        progress: &Mutex<Progress>,
    ) -> std::result::Result<(), Stop> {
        let p = &self.pack;
        let part = part_path(&p.dir, f);
        let mut have = stamp(&part).map(|s| s.size).unwrap_or(0);
        if have > f.size {
            let _ = std::fs::remove_file(&part);
            progress.lock().received -= have.min(f.size);
            have = 0;
        }
        if have < f.size {
            let resp = agent
                .get(url)
                .header("Range", format!("bytes={have}-"))
                .header("Accept-Encoding", "identity")
                .call()
                .map_err(|e| failed("offline", e.to_string()))?;
            let status = resp.status().as_u16();
            let append = match status {
                206 => true,
                200 => false,
                416 => {
                    // The server has nothing past `have`: start over.
                    let _ = std::fs::remove_file(&part);
                    progress.lock().received -= have;
                    return Err(failed("internal", format!("HTTP 416 for {} at {have}", f.file)));
                }
                404 | 403 | 401 | 410 => return Err(failed("notFound", format!("HTTP {status} for {url}"))),
                s => return Err(failed("internal", format!("HTTP {s} for {url}"))),
            };
            if !append && have > 0 {
                progress.lock().received -= have;
                have = 0;
            }
            let mut out = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(append)
                .truncate(!append)
                .open(&part)
                .map_err(|e| io_stop(&part, e))?;
            let mut body = resp.into_body().into_reader();
            let mut buf = vec![0u8; READ_CHUNK];
            loop {
                if p.cancel.load(Ordering::SeqCst) {
                    let _ = out.flush();
                    return Err(Stop::Cancelled);
                }
                let n = match body.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(e) => {
                        let _ = out.flush();
                        return Err(failed("offline", e.to_string()));
                    }
                };
                let n = n.min((f.size - have) as usize);
                out.write_all(&buf[..n]).map_err(|e| io_stop(&part, e))?;
                have += n as u64;
                let state = {
                    let mut pr = progress.lock();
                    pr.add(n as u64);
                    ModelPackState::Downloading {
                        received_bytes: pr.received.min(pr.total),
                        total_bytes: pr.total,
                        bytes_per_second: pr.rate.map(|r| r.round()),
                    }
                };
                self.set_pack_state(state, true);
                if have >= f.size {
                    break;
                }
            }
            out.flush().map_err(|e| io_stop(&part, e))?;
            drop(out);
            if have < f.size {
                return Err(failed("offline", format!("{}: connection closed at {have} of {} bytes", f.file, f.size)));
            }
        }
        let sha = cull_ml::sha256_file(&part).map_err(|e| io_stop(&part, e))?;
        if !sha.eq_ignore_ascii_case(&f.sha256) {
            let _ = std::fs::remove_file(&part);
            progress.lock().received -= f.size;
            return Err(failed("checksum", format!("{} does not match its checksum", f.file)));
        }
        let dest = p.dir.join(&f.file);
        std::fs::rename(&part, &dest).map_err(|e| io_stop(&dest, e))?;
        Ok(())
    }

    /// Load every model once and check its results (worker thread), then
    /// `ready`. A file that fails its checksum is deleted, so a new
    /// download fetches it again.
    fn prepare_model_pack(&self) {
        let p = &self.pack;
        self.set_pack_state(ModelPackState::Preparing { progress: 0.0 }, false);
        let opts = self.ml_options();
        let models = match MaximumModels::load(&p.dir, &opts) {
            Ok(m) => m,
            Err(e) => return self.pack_load_failed(e),
        };
        self.set_pack_state(ModelPackState::Preparing { progress: 0.5 }, false);
        let check = models.check(&|f| self.set_pack_state(ModelPackState::Preparing { progress: 0.5 + 0.5 * f }, true));
        drop(models);
        if let Err(e) = check {
            return self.pack_failed("internal", e.to_string());
        }
        if let Some(stamps) = p.stamps() {
            if let Ok(j) = serde_json::to_string(&stamps) {
                let _ = self.kv_set(READY_KEY, &j);
            }
        }
        self.set_pack_state(ModelPackState::Ready, false);
    }

    fn pack_load_failed(&self, e: cull_ml::MlError) {
        let p = &self.pack;
        match &e {
            cull_ml::MlError::Checksum(file) => {
                let name = file.split(" (").next().unwrap_or(file);
                if let Some(f) = p.files.iter().find(|f| f.file == name) {
                    let _ = std::fs::remove_file(p.dir.join(&f.file));
                }
                let _ = self.kv_delete(READY_KEY);
                self.pack_failed("checksum", e.to_string())
            }
            cull_ml::MlError::MissingModel(_) => {
                let _ = self.kv_delete(READY_KEY);
                self.pack_failed("notFound", e.to_string())
            }
            _ => self.pack_failed("internal", e.to_string()),
        }
    }

    /// The pack's models for an analysis in Maximum precision, loaded on
    /// first use; `None` when the pack is not ready or fails to load (the
    /// analysis then runs in Standard).
    pub(crate) fn maximum_models(&self) -> Option<Arc<MaximumModels>> {
        if !self.pack.is_ready() {
            return None;
        }
        let mut slot = self.pack.models.lock();
        if let Some(m) = slot.as_ref() {
            return Some(m.clone());
        }
        match MaximumModels::load(&self.pack.dir, &self.ml_options()) {
            Ok(m) => {
                let m = Arc::new(m);
                *slot = Some(m.clone());
                Some(m)
            }
            Err(e) => {
                drop(slot);
                self.pack_load_failed(e);
                None
            }
        }
    }

    /// Free the pack's models (after an analysis): ~1-2 GB of memory.
    pub(crate) fn release_maximum_models(&self) {
        *self.pack.models.lock() = None;
    }

    /// The CLIP L/14 text tower for searching a project analysed in Maximum
    /// precision, loaded on first use and kept.
    pub(crate) fn maximum_text(&self) -> Option<Arc<MaximumText>> {
        if !self.pack.is_ready() {
            return None;
        }
        let mut slot = self.pack.text.lock();
        if let Some(t) = slot.as_ref() {
            return Some(t.clone());
        }
        match MaximumText::load(&self.pack.dir, &self.ml_options()) {
            Ok(t) => {
                let t = Arc::new(t);
                *slot = Some(t.clone());
                Some(t)
            }
            Err(e) => {
                log::warn!("Maximum pack text tower: {e}");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_serialise_as_the_contract() {
        let v = serde_json::to_value(ModelPackState::Downloading {
            received_bytes: 5,
            total_bytes: 9,
            bytes_per_second: None,
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "status": "downloading", "receivedBytes": 5, "totalBytes": 9, "bytesPerSecond": null })
        );
        let v = serde_json::to_value(ModelPackState::Failed {
            code: "cancelled".into(),
            message: "m".into(),
            received_bytes: 1,
            total_bytes: 2,
        })
        .unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "status": "failed", "code": "cancelled", "message": "m", "receivedBytes": 1, "totalBytes": 2 })
        );
        assert_eq!(serde_json::to_value(ModelPackState::Preparing { progress: 0.5 }).unwrap()["progress"], 0.5);
        assert_eq!(serde_json::to_value(ModelPackState::Absent).unwrap(), serde_json::json!({ "status": "absent" }));
        let pack =
            ModelPack { id: "maximum", size_bytes: 3, disk_bytes: 3, free_bytes: None, state: ModelPackState::Ready };
        let v = serde_json::to_value(pack).unwrap();
        assert_eq!(
            v,
            serde_json::json!({ "id": "maximum", "sizeBytes": 3, "diskBytes": 3, "freeBytes": null, "state": { "status": "ready" } })
        );
    }

    #[test]
    fn free_space_is_known_here() {
        assert!(free_space(&std::env::temp_dir().join("does/not/exist")).is_some_and(|b| b > 0));
    }

    #[test]
    fn pack_files_come_from_the_manifest() {
        let p = PackManager::new(PathBuf::from("/tmp/x/models/maximum"));
        assert_eq!(p.files.len(), 5);
        assert!(p.total() > 700_000_000);
        assert!(p.dir.ends_with("models/maximum"));
    }

    /// A tiny HTTP/1.1 server with range support serving `files`: the first
    /// response of `drop_once` is cut after `drop_after` bytes, `corrupt`
    /// is served with its bytes flipped, every chunk waits `pause`.
    struct Server {
        port: u16,
        ranges: Arc<Mutex<Vec<(String, u64)>>>,
    }

    fn serve(
        files: Vec<(&'static str, Vec<u8>)>,
        drop_once: Option<(&'static str, usize)>,
        corrupt: Option<&'static str>,
        pause: Duration,
    ) -> Server {
        use std::io::{BufRead, BufReader};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let ranges = Arc::new(Mutex::new(Vec::new()));
        let seen = ranges.clone();
        let dropped = Arc::new(AtomicBool::new(false));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (files, seen, dropped) = (files.clone(), seen.clone(), dropped.clone());
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut out = stream;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 {
                            return;
                        }
                        let path = line.split_whitespace().nth(1).unwrap_or("/").trim_start_matches('/').to_string();
                        let mut start = 0u64;
                        loop {
                            let mut h = String::new();
                            if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                                break;
                            }
                            if let Some(v) = h.to_ascii_lowercase().strip_prefix("range: bytes=") {
                                start = v.trim().trim_end_matches('-').split('-').next().unwrap().parse().unwrap_or(0);
                            }
                        }
                        seen.lock().push((path.clone(), start));
                        let Some((_, data)) = files.iter().find(|(n, _)| *n == path) else {
                            let _ = out.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n");
                            continue;
                        };
                        let body: Vec<u8> = data[start as usize..]
                            .iter()
                            .map(|b| if corrupt == Some(path.as_str()) { !b } else { *b })
                            .collect();
                        let head = format!(
                            "HTTP/1.1 206 Partial Content\r\ncontent-length: {}\r\ncontent-range: bytes {start}-{}/{}\r\n\r\n",
                            body.len(),
                            data.len() - 1,
                            data.len()
                        );
                        let _ = out.write_all(head.as_bytes());
                        for (k, chunk) in body.chunks(8192).enumerate() {
                            if let Some((name, after)) = drop_once {
                                if name == path && k * 8192 >= after && !dropped.swap(true, Ordering::SeqCst) {
                                    return; // connection closed mid-body
                                }
                            }
                            if out.write_all(chunk).is_err() {
                                return;
                            }
                            std::thread::sleep(pause);
                        }
                    }
                });
            }
        });
        Server { port, ranges }
    }

    fn pack_file(server: &Server, name: &str, data: &[u8]) -> PackFile {
        PackFile {
            id: name.into(),
            file: name.into(),
            url: format!("http://127.0.0.1:{}/{name}", server.port),
            mirrors: vec![],
            sha256: crate::sha256_hex_for_tests(data),
            size: data.len() as u64,
        }
    }

    fn engine_with(files: Vec<PackFile>) -> (tempfile::TempDir, Arc<Engine>) {
        let tmp = tempfile::tempdir().unwrap();
        let mut engine = Engine::open(crate::EngineConfig {
            data_dir: tmp.path().join("data"),
            cache_dir: tmp.path().join("cache"),
            models_dir: tmp.path().join("no-models"),
            pack_dir: None,
            url_mapper: Arc::new(|p: &Path| p.display().to_string()),
            events: Arc::new(crate::NullSink),
            secrets: None,
        })
        .unwrap();
        let e = Arc::get_mut(&mut engine).expect("no other reference yet");
        e.pack.files = files;
        e.pack.retry_delay = Duration::from_millis(10);
        std::fs::create_dir_all(&e.pack.dir).unwrap();
        (tmp, engine)
    }

    fn data(n: usize, seed: u8) -> Vec<u8> {
        (0..n).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
    }

    #[test]
    fn an_interrupted_file_resumes_where_it_stopped() {
        let (a, b) = (data(300_000, 1), data(50_000, 2));
        let server =
            serve(vec![("a.onnx", a.clone()), ("b.onnx", b.clone())], Some(("a.onnx", 100_000)), None, Duration::ZERO);
        let (_tmp, engine) = engine_with(vec![pack_file(&server, "a.onnx", &a), pack_file(&server, "b.onnx", &b)]);
        engine.download_files().expect("download");
        let dir = &engine.pack.dir;
        assert_eq!(std::fs::read(dir.join("a.onnx")).unwrap(), a);
        assert_eq!(std::fs::read(dir.join("b.onnx")).unwrap(), b);
        assert!(!dir.join("a.onnx.part").exists());
        let ranges = server.ranges.lock().clone();
        let resumed: Vec<u64> = ranges.iter().filter(|(n, _)| n == "a.onnx").map(|r| r.1).collect();
        assert_eq!(resumed.len(), 2, "{ranges:?}");
        assert!(resumed[1] >= 90_000, "resumed from the part, not from 0: {ranges:?}");
        assert!(engine.pack.complete());
    }

    #[test]
    fn a_corrupt_file_fails_with_checksum_and_leaves_no_part() {
        let a = data(40_000, 3);
        let server = serve(vec![("a.onnx", a.clone())], None, Some("a.onnx"), Duration::ZERO);
        let (_tmp, engine) = engine_with(vec![pack_file(&server, "a.onnx", &a)]);
        match engine.download_files() {
            Err(Stop::Failed { code: "checksum", .. }) => {}
            other => panic!("{other:?}"),
        }
        assert_eq!(server.ranges.lock().len(), ATTEMPTS_PER_URL, "each attempt starts the file over");
        assert!(!engine.pack.dir.join("a.onnx.part").exists() && !engine.pack.dir.join("a.onnx").exists());
        // A missing file (404, or 401 for a missing Hugging Face repository)
        // fails at once: one request per URL, no retry.
        let (_tmp, engine) = engine_with(vec![PackFile {
            url: format!("http://127.0.0.1:{}/zzz", server.port),
            mirrors: vec![format!("http://127.0.0.1:{}/yyy", server.port)],
            ..pack_file(&server, "a.onnx", &a)
        }]);
        let before = server.ranges.lock().len();
        match engine.download_files() {
            Err(Stop::Failed { code: "notFound", .. }) => {}
            other => panic!("{other:?}"),
        }
        let asked: Vec<String> = server.ranges.lock()[before..].iter().map(|r| r.0.clone()).collect();
        assert_eq!(asked, ["zzz", "yyy"]);
    }

    #[test]
    fn cancel_keeps_the_part_and_reports_cancelled() {
        let a = data(400_000, 4);
        let server = serve(vec![("a.onnx", a.clone())], None, None, Duration::from_millis(5));
        let (_tmp, engine) = engine_with(vec![pack_file(&server, "a.onnx", &a)]);
        *engine.pack.state.lock() =
            ModelPackState::Downloading { received_bytes: 0, total_bytes: 1, bytes_per_second: None };
        let e2 = engine.clone();
        let worker = std::thread::spawn(move || e2.download_files());
        let part = engine.pack.dir.join("a.onnx.part");
        let t = Instant::now();
        while stamp(&part).is_none_or(|s| s.size < 50_000) && t.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(5));
        }
        let pack = engine.cancel_model_pack();
        assert!(matches!(worker.join().unwrap(), Err(Stop::Cancelled)));
        let kept = stamp(&part).unwrap().size;
        assert!(kept >= 50_000 && kept < a.len() as u64);
        match pack.state {
            ModelPackState::Failed { code, received_bytes, total_bytes, .. } => {
                assert_eq!(code, "cancelled");
                assert!(received_bytes >= 50_000 && total_bytes == a.len() as u64);
            }
            other => panic!("{other:?}"),
        }
        // The engine reopened on the same folder offers to resume.
        engine.pack.cancel.store(false, Ordering::SeqCst);
        engine.download_files().expect("resume");
        assert_eq!(std::fs::read(engine.pack.dir.join("a.onnx")).unwrap(), a);
        assert!(server.ranges.lock().last().unwrap().1 >= 50_000);
    }

    /// What an app update waits for (`Engine::busy`).
    #[test]
    fn the_engine_is_busy_while_a_job_runs() {
        let (_tmp, engine) = engine_with(vec![]);
        assert!(!engine.busy());
        for state in [
            ModelPackState::Downloading { received_bytes: 0, total_bytes: 1, bytes_per_second: None },
            ModelPackState::Preparing { progress: 0.5 },
        ] {
            *engine.pack.state.lock() = state;
            assert!(engine.busy());
        }
        *engine.pack.state.lock() = ModelPackState::Ready;
        assert!(!engine.busy());
        {
            let _export = crate::Counted::new(&engine.exports);
            assert!(engine.busy());
        }
        assert!(!engine.busy());
        for jobs in [&engine.running, &engine.uploads] {
            jobs.lock().insert("p".into(), Arc::new(AtomicBool::new(false)));
            assert!(engine.busy());
            drop(crate::Running(jobs, "p"));
            assert!(!engine.busy());
        }
    }
}
