//! Pelli Instant engine.
//!
//! Owns the library (SQLite), the preview cache, the analysis pipeline and
//! the decisions. Hosts (the Tauri desktop shell, the `pelli-cull` CLI and its
//! HTTP dev bridge) call [`Engine`] methods — one per command of
//! `src/lib/contract.ts` — and receive events through an [`EventSink`].
//!
//! Threading: every method is blocking and thread-safe; long jobs (analysis,
//! upload) run on their own threads and report through events.
//!
//! # Pelli account and monthly quota
//!
//! Everything runs on the photographer's computer. Sorting needs a Pelli
//! account: the photos a project analyses for the first time are counted on
//! the monthly quota of the Pelli plan ([`usage`]); re-analyses, review and
//! export never count. Sending photos to a Pelli gallery also needs the
//! account ([`upload`]).
//!
//! # No-account build
//!
//! Developers who compile the app themselves can build it without the Pelli
//! account:
//!
//! ```sh
//! PELLI_INSTANT_NO_ACCOUNT=1 pnpm tauri build    # or `pnpm tauri dev`, `cargo build`
//! ```
//!
//! (`1`, `true` or `yes`). Such a build needs no sign-in to analyse and never
//! calls the usage API (no reservation, no quota); everything else is
//! identical, sending to a Pelli gallery still asks to sign in. The UI learns
//! it from `app_info` (`noAccount`). The variable is read at compile time
//! ([`no_account_build`]); cargo rebuilds this crate when it changes.
//!
//! In the same spirit, a release build talks to the Pelli chosen when it was
//! compiled (`PELLI_API_URL`, default `https://app.pelli.io`) and ignores
//! `PELLI_API_URL` at run time; only debug builds (`pnpm tauri dev`, `pnpm
//! app:fake-pelli`) can be pointed at another Pelli, such as
//! `scripts/fake-pelli.mjs`, without rebuilding ([`pelli::api_url`]).

pub mod account;
pub mod commands;
pub mod export;
pub mod modelpack;
pub mod pelli;
pub mod pipeline;
pub mod query;
pub mod settings;
pub mod store;
pub mod system;
pub mod upload;
mod uploader;
pub mod usage;
pub mod view;

use crate::pipeline::{AnalysisJob, OwnedFacts};
use crate::store::{AnalysisRow, ProjectRow, Store};
use crate::usage::QuotaDetails;
use crate::view::*;
use cull_types::{final_decision, Assessment, CullSettings, FileKind, PhotoAsset, Scene, SimilarGroup, UserMark};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

pub use cull_analysis;
pub use cull_decode;
pub use cull_ml;
pub use cull_types;

/// `true` when built with `PELLI_INSTANT_NO_ACCOUNT` set to `1`, `true` or
/// `yes`: analyses need no Pelli account and are never counted (see the
/// crate docs).
pub fn no_account_build() -> bool {
    option_env!("PELLI_INSTANT_NO_ACCOUNT")
        .is_some_and(|v| ["1", "true", "yes"].iter().any(|on| v.trim().eq_ignore_ascii_case(on)))
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("i/o error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Decode(#[from] cull_decode::DecodeError),
    #[error("{0}")]
    Analysis(#[from] cull_analysis::AnalysisError),
    #[error("{0}")]
    Ml(#[from] cull_ml::MlError),
    #[error("project not found: {0}")]
    ProjectNotFound(String),
    #[error("photo not found: {0}")]
    PhotoNotFound(String),
    #[error("folder not found: {0}")]
    FolderNotFound(String),
    #[error("an analysis is already running for this project")]
    AlreadyRunning,
    #[error("no supported photo in this folder")]
    NoPhotos,
    #[error("cancelled")]
    Cancelled,
    #[error("natural-language search needs the CLIP models")]
    SearchUnavailable,
    /// The monthly quota left is too small for the new photos.
    #[error("monthly quota exceeded: {} new photo(s) to analyse", .0.needed)]
    QuotaExceeded(QuotaDetails),
    /// Pelli is unreachable and the plan has a monthly quota: the new photos
    /// cannot be counted.
    #[error("Pelli is unreachable: {} new photo(s) cannot be counted on the monthly quota", .0.needed)]
    QuotaOffline(QuotaDetails),
    /// Not enough free disk space for the Maximum pack.
    #[error("not enough disk space: {needed} bytes needed, {free} free")]
    DiskFull { needed: u64, free: u64 },
    /// Pelli API error (snake_case code from Pelli, or one of the engine's
    /// own: `network`, `bad_response`, `signed_out`…).
    #[error("{code}: {message}")]
    Remote { code: String, message: String },
}

impl EngineError {
    pub fn io(path: &Path, source: std::io::Error) -> Self {
        EngineError::Io { path: path.display().to_string(), source }
    }

    /// Stable machine code for the UI (`ErrorCode` in the contract).
    pub fn code(&self) -> &str {
        use std::io::ErrorKind;
        fn io_code(e: &std::io::Error) -> &'static str {
            match e.kind() {
                ErrorKind::PermissionDenied => "permissionDenied",
                ErrorKind::NotFound => "notFound",
                _ => "internal",
            }
        }
        match self {
            EngineError::Io { source, .. } => io_code(source),
            EngineError::Decode(cull_decode::DecodeError::Io { source, .. }) => io_code(source),
            EngineError::Decode(_) => "internal",
            EngineError::Db(_) | EngineError::Json(_) | EngineError::Analysis(_) => "internal",
            EngineError::Ml(cull_ml::MlError::MissingModel(_)) | EngineError::Ml(cull_ml::MlError::Checksum(_)) => {
                "modelsMissing"
            }
            EngineError::Ml(_) => "internal",
            EngineError::ProjectNotFound(_) | EngineError::PhotoNotFound(_) | EngineError::FolderNotFound(_) => {
                "notFound"
            }
            EngineError::NoPhotos => "noPhotos",
            EngineError::AlreadyRunning => "alreadyRunning",
            EngineError::SearchUnavailable => "modelsMissing",
            EngineError::Cancelled => "cancelled",
            EngineError::QuotaExceeded(_) => "quotaExceeded",
            EngineError::QuotaOffline(_) => "quotaOffline",
            EngineError::DiskFull { .. } => "diskFull",
            EngineError::Remote { code, .. } => match code.as_str() {
                "network" => "offline",
                "unauthorized" | "missing_token" | "invalid_token" | "account_unavailable" | "signed_out" => {
                    "unauthorized"
                }
                other => other,
            },
        }
    }

    /// Structured details for the UI (`details` next to `code`), if any.
    pub fn details(&self) -> Option<serde_json::Value> {
        match self {
            EngineError::QuotaExceeded(d) | EngineError::QuotaOffline(d) => serde_json::to_value(d).ok(),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, EngineError>;

// ---------------------------------------------------------------------------
// Host integration
// ---------------------------------------------------------------------------

/// Receives engine events (names and payloads of `CullEvents` in the
/// contract). Implementations must be cheap and non-blocking.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &str, payload: serde_json::Value);
}

/// Sink that drops everything (CLI batch mode, tests).
pub struct NullSink;
impl EventSink for NullSink {
    fn emit(&self, _: &str, _: serde_json::Value) {}
}

/// Turns a cache/original file path into a URL the UI can load
/// (Tauri asset protocol, HTTP bridge…).
pub type UrlMapper = Arc<dyn Fn(&Path) -> String + Send + Sync>;

pub struct EngineConfig {
    /// Library database and settings.
    pub data_dir: PathBuf,
    /// Thumbnails and previews (safe to delete).
    pub cache_dir: PathBuf,
    /// ONNX models (see `models/manifest.json`).
    pub models_dir: PathBuf,
    /// Folder of the optional Maximum pack; `None` = `<data_dir>/models/maximum`
    /// (the CLI points it at a prepared folder for benchmarks).
    pub pack_dir: Option<PathBuf>,
    pub url_mapper: UrlMapper,
    pub events: Arc<dyn EventSink>,
    /// Keeps the Pelli access token (OS keychain on desktop). `None` = the
    /// local library.
    pub secrets: Option<Arc<dyn account::SecretStore>>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn sha1_hex(s: &str) -> String {
    sha1_smol::Sha1::from(s).digest().to_string()
}

/// "NIKON CORPORATION" + "NIKON D750" → "NIKON D750"; "Canon" + "Canon EOS R5"
/// → "Canon EOS R5"; "SONY" + "ILCE-7M3" → "SONY ILCE-7M3".
pub fn camera_name(make: Option<&str>, model: Option<&str>) -> Option<String> {
    let make = make.map(str::trim).filter(|s| !s.is_empty());
    let model = model.map(str::trim).filter(|s| !s.is_empty());
    match (make, model) {
        (Some(make), Some(model)) => {
            let brand = make.split_whitespace().next().unwrap_or(make);
            if model.to_lowercase().starts_with(&brand.to_lowercase()) {
                Some(model.to_string())
            } else {
                Some(format!("{brand} {model}"))
            }
        }
        (None, Some(model)) => Some(model.to_string()),
        (Some(make), None) => Some(make.to_string()),
        (None, None) => None,
    }
}

/// Thumbnail long edge (grid cards up to ~360 px wide on Retina).
pub const THUMB_EDGE: u32 = 480;
/// Minimum long edge decoded for the analysis.
pub const ANALYSIS_EDGE: u32 = 1440;

// ---------------------------------------------------------------------------
// In-memory state of an open project
// ---------------------------------------------------------------------------

/// SHA-256 (hex) of bytes, for tests.
#[cfg(test)]
pub(crate) fn sha256_hex_for_tests(data: &[u8]) -> String {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("f");
    std::fs::write(&p, data).unwrap();
    cull_ml::sha256_file(&p).unwrap()
}

#[cfg(test)]
mod tests {
    #[test]
    fn camera_names() {
        use super::camera_name;
        assert_eq!(camera_name(Some("NIKON CORPORATION"), Some("NIKON D750")).as_deref(), Some("NIKON D750"));
        assert_eq!(camera_name(Some("Canon"), Some("Canon EOS R6")).as_deref(), Some("Canon EOS R6"));
        assert_eq!(camera_name(Some("SONY"), Some("ILCE-7M3")).as_deref(), Some("SONY ILCE-7M3"));
        assert_eq!(
            camera_name(Some("OLYMPUS IMAGING CORP."), Some("E-M1MarkII")).as_deref(),
            Some("OLYMPUS E-M1MarkII")
        );
        assert_eq!(camera_name(None, None), None);
    }
}

#[derive(Default)]
pub(crate) struct ProjectState {
    pub(crate) assets: Vec<PhotoAsset>,
    pub(crate) analyses: HashMap<String, AnalysisRow>,
    pub(crate) marks: HashMap<String, UserMark>,
    pub(crate) assessments: HashMap<String, Assessment>,
    pub(crate) groups: Vec<SimilarGroup>,
    pub(crate) scenes: Vec<Scene>,
    pub(crate) scene_of: HashMap<String, u32>,
}

impl ProjectState {
    fn facts(&self) -> Vec<OwnedFacts> {
        self.assets
            .iter()
            .filter_map(|a| {
                let row = self.analyses.get(&a.id)?;
                OwnedFacts::from_row(row)
            })
            .collect()
    }

    /// Re-run grouping, scenes and decisions on the cached measures.
    pub(crate) fn decide(&mut self, settings: &CullSettings) {
        let owned = self.facts();
        let facts: Vec<_> = owned.iter().map(OwnedFacts::as_facts).collect();
        self.groups = cull_analysis::group_similar(&facts, settings);
        self.scenes = cull_analysis::split_scenes(&facts, settings);
        let assessments = cull_analysis::assess(&facts, &self.groups, settings);
        self.assessments = owned.iter().map(|f| f.id.clone()).zip(assessments).collect();
        self.scene_of = self.scenes.iter().flat_map(|s| s.photo_ids.iter().map(move |id| (id.clone(), s.id))).collect();
    }

    fn counts(&self) -> DecisionCounts {
        let mut c = DecisionCounts::default();
        for a in &self.assets {
            let ai = self.assessments.get(&a.id).map(|x| x.verdict);
            c.add(final_decision(ai, &self.marks.get(&a.id).copied().unwrap_or_default()));
        }
        c
    }
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// Removes a project's job (analysis, upload) from its table when the job
/// thread ends, panic included.
pub(crate) struct Running<'a>(pub(crate) &'a Mutex<HashMap<String, Arc<AtomicBool>>>, pub(crate) &'a str);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.lock().remove(self.1);
    }
}

/// Counts a job that runs on its caller's thread (an export) until dropped.
pub(crate) struct Counted<'a>(&'a AtomicUsize);

impl<'a> Counted<'a> {
    pub(crate) fn new(count: &'a AtomicUsize) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Counted(count)
    }
}

impl Drop for Counted<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

pub struct Engine {
    pub(crate) config: EngineConfig,
    pub(crate) store: Mutex<Store>,
    pub(crate) models: RwLock<Option<Arc<cull_ml::ModelSet>>>,
    models_error: RwLock<Option<String>>,
    pub(crate) states: Mutex<HashMap<String, Arc<Mutex<ProjectState>>>>,
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub(crate) settings: RwLock<settings::AppSettings>,
    pub(crate) secrets: Option<Arc<dyn account::SecretStore>>,
    pub(crate) uploads: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Exports under way ([`Counted`]).
    pub(crate) exports: AtomicUsize,
    pub(crate) pack: modelpack::PackManager,
    /// Analysis worker threads forced by the host (CLI benchmarks), instead
    /// of the count derived from the settings.
    worker_override: RwLock<Option<usize>>,
}

impl Engine {
    pub fn open(config: EngineConfig) -> Result<Arc<Engine>> {
        std::fs::create_dir_all(&config.cache_dir).map_err(|e| EngineError::io(&config.cache_dir, e))?;
        let store = Store::open(&config.data_dir.join("library.sqlite"))?;
        let settings = settings::AppSettings::load(&store, &config.cache_dir);
        let secrets = config.secrets.clone();
        let pack = modelpack::PackManager::new(
            config.pack_dir.clone().unwrap_or_else(|| config.data_dir.join("models").join("maximum")),
        );
        let engine = Arc::new(Engine {
            secrets,
            pack,
            worker_override: RwLock::new(None),
            config,
            store: Mutex::new(store),
            models: RwLock::new(None),
            models_error: RwLock::new(None),
            states: Mutex::new(HashMap::new()),
            running: Mutex::new(HashMap::new()),
            uploads: Mutex::new(HashMap::new()),
            exports: AtomicUsize::new(0),
            settings: RwLock::new(settings),
        });
        engine.reload_models();
        engine.init_model_pack();
        Ok(engine)
    }

    /// ONNX Runtime options from the settings.
    pub(crate) fn ml_options(&self) -> cull_ml::RuntimeOptions {
        let acceleration = if self.settings.read().acceleration == settings::AccelerationSetting::Cpu {
            cull_ml::Acceleration::CpuOnly
        } else {
            cull_ml::Acceleration::Auto
        };
        cull_ml::RuntimeOptions {
            acceleration,
            // Never write next to the models (they may be in a signed bundle).
            checksum_cache_dir: Some(self.config.data_dir.clone()),
            accelerator_cache_dir: Some(self.config.cache_dir.join("ml-compiled")),
            memory_bytes: system::total_memory_bytes(),
            ..Default::default()
        }
    }

    /// Analysis worker threads: the host's override, else from the settings.
    pub fn worker_threads(&self) -> usize {
        self.worker_override.read().unwrap_or_else(|| self.settings.read().worker_threads()).clamp(1, 64)
    }

    /// Force the number of analysis worker threads (`None`: from the
    /// settings). Not stored: for benchmarks.
    pub fn set_worker_threads(&self, n: Option<usize>) {
        *self.worker_override.write() = n.filter(|&n| n > 0);
    }

    /// (Re)load the ONNX models with the current acceleration settings.
    /// Failures are kept and reported by [`Engine::models_status`]; the
    /// engine keeps working without ML (technical measures only).
    pub fn reload_models(&self) {
        let opts = self.ml_options();
        self.release_maximum_models();
        match cull_ml::ModelSet::load(&self.config.models_dir, &opts) {
            Ok(m) => {
                let m = Arc::new(m);
                *self.models.write() = Some(m.clone());
                *self.models_error.write() = None;
                // Create the sessions of the analysis workers now, in the
                // background, so the first analysis does not wait for them.
                let workers = self.worker_threads();
                let precision = self.settings.read().precision;
                let spawned = std::thread::Builder::new()
                    .name("models-prepare".into())
                    .spawn(move || m.prepare_for(workers, precision));
                if let Err(e) = spawned {
                    log::warn!("cannot prepare the models in the background: {e}");
                }
            }
            Err(e) => {
                log::warn!("models unavailable: {e}");
                *self.models.write() = None;
                *self.models_error.write() = Some(e.to_string());
            }
        }
    }

    pub fn models(&self) -> Option<Arc<cull_ml::ModelSet>> {
        self.models.read().clone()
    }

    /// `(capabilities, load error)`.
    pub fn models_status(&self) -> (cull_ml::Capabilities, Option<String>) {
        let caps = self.models().map(|m| m.capabilities()).unwrap_or_default();
        (caps, self.models_error.read().clone())
    }

    pub fn events(&self) -> &Arc<dyn EventSink> {
        &self.config.events
    }

    pub fn url_for(&self, path: &Path) -> String {
        (self.config.url_mapper)(path)
    }

    pub fn cache_dir(&self) -> &Path {
        &self.config.cache_dir
    }

    pub(crate) fn project_cache_dir(&self, project_id: &str) -> PathBuf {
        self.config.cache_dir.join(project_id)
    }

    // -- projects ----------------------------------------------------------

    pub fn list_projects(&self) -> Result<Vec<ProjectSummary>> {
        let rows = self.store.lock().projects()?;
        rows.iter().map(|p| self.summary(p)).collect()
    }

    /// Scan a folder and create (or reopen) its project. A folder without
    /// photo is `NoPhotos` and changes nothing: a project opened before
    /// (its folder now empty, unmounted or scanned without its subfolders)
    /// keeps its photos, marks and analyses.
    pub fn open_folder(&self, path: &Path, recursive: bool) -> Result<ProjectSummary> {
        let root = std::fs::canonicalize(path).map_err(|_| EngineError::FolderNotFound(path.display().to_string()))?;
        if !root.is_dir() {
            return Err(EngineError::FolderNotFound(root.display().to_string()));
        }
        let key = root.to_string_lossy().to_string();
        let id = sha1_hex(&if cfg!(any(target_os = "windows", target_os = "macos")) {
            key.to_lowercase()
        } else {
            key.clone()
        })[..16]
            .to_string();
        let opts = cull_decode::ScanOptions { recursive, ..Default::default() };
        let assets = cull_decode::scan_folder(&root, &opts)?;
        if assets.is_empty() {
            return Err(EngineError::NoPhotos);
        }
        let now = now_ms();
        let defaults = self.settings.read().cull.clone();
        let mut row = {
            let mut store = self.store.lock();
            let existing = store.project(&id)?;
            let row = ProjectRow {
                id: id.clone(),
                name: root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| key.clone()),
                root_path: root.clone(),
                recursive,
                created_at: existing.as_ref().map(|e| e.created_at).unwrap_or(now),
                last_opened_at: now,
                analyzed_at: existing.as_ref().and_then(|e| e.analyzed_at),
                precision: existing.as_ref().and_then(|e| e.precision),
                settings: existing.map(|e| e.settings).unwrap_or_else(|| defaults.clone()),
            };
            store.upsert_project(&row)?;
            store.sync_photos(&id, &assets)?;
            row
        };
        self.states.lock().remove(&id);
        let state = self.state(&id)?;
        // A project that never got a photo analysed is still a new one: it
        // follows the current defaults (they may have changed since).
        let untouched = row.analyzed_at.is_none() && state.lock().analyses.is_empty() && !self.is_running(&id);
        drop(state);
        if untouched && row.settings != defaults {
            row.settings = defaults;
            self.store.lock().upsert_project(&row)?;
        }
        self.summary(&row)
    }

    pub fn remove_project(&self, project_id: &str) -> Result<()> {
        self.cancel_analysis(project_id);
        self.store.lock().delete_project(project_id)?;
        self.states.lock().remove(project_id);
        let dir = self.project_cache_dir(project_id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| EngineError::io(&dir, e))?;
        }
        Ok(())
    }

    pub(crate) fn project_row(&self, project_id: &str) -> Result<ProjectRow> {
        self.store.lock().project(project_id)?.ok_or_else(|| EngineError::ProjectNotFound(project_id.to_string()))
    }

    /// Loaded state of a project (built from the database on first access).
    pub(crate) fn state(&self, project_id: &str) -> Result<Arc<Mutex<ProjectState>>> {
        if let Some(s) = self.states.lock().get(project_id) {
            return Ok(s.clone());
        }
        let row = self.project_row(project_id)?;
        let (assets, analyses, marks) = {
            let store = self.store.lock();
            (store.photos(project_id)?, store.analyses(project_id)?, store.marks(project_id)?)
        };
        let mut state = ProjectState {
            assets,
            analyses: analyses.into_iter().map(|a| (a.photo_id.clone(), a)).collect(),
            marks: marks.into_iter().collect(),
            ..Default::default()
        };
        state.decide(&row.settings);
        let state = Arc::new(Mutex::new(state));
        self.states.lock().insert(project_id.to_string(), state.clone());
        Ok(state)
    }

    fn summary(&self, p: &ProjectRow) -> Result<ProjectSummary> {
        let state = self.state(&p.id)?;
        let st = state.lock();
        let cover = st
            .assets
            .iter()
            .filter(|a| st.assessments.get(&a.id).map(|x| x.verdict) == Some(cull_types::Verdict::Keep))
            .chain(st.assets.iter())
            .find_map(|a| st.analyses.get(&a.id).and_then(|r| r.thumb_path.clone()))
            .filter(|p| p.exists());
        Ok(ProjectSummary {
            id: p.id.clone(),
            name: p.name.clone(),
            root_path: p.root_path.to_string_lossy().into_owned(),
            photo_count: st.assets.len() as u32,
            raw_count: st.assets.iter().filter(|a| a.primary.kind == FileKind::Raw).count() as u32,
            created_at: p.created_at,
            last_opened_at: p.last_opened_at,
            analyzed_at: p.analyzed_at,
            cover_thumb_url: cover.map(|c| self.url_for(&c)),
            counts: st.counts(),
            settings: p.settings.clone(),
            precision: p.precision,
        })
    }

    pub fn get_project(&self, project_id: &str) -> Result<ProjectData> {
        let row = self.project_row(project_id)?;
        let summary = self.summary(&row)?;
        let state = self.state(project_id)?;
        let st = state.lock();
        let photos = st.assets.iter().map(|a| self.photo_view(&st, a)).collect();
        Ok(ProjectData { project: summary, photos, groups: st.groups.clone(), scenes: st.scenes.clone() })
    }

    pub(crate) fn photo_view(&self, st: &ProjectState, a: &PhotoAsset) -> PhotoView {
        let row = st.analyses.get(&a.id);
        let mark = st.marks.get(&a.id).copied().unwrap_or_default();
        let assessment = st.assessments.get(&a.id).cloned();
        let exif = row.and_then(|r| r.exif.clone()).unwrap_or_default();
        let all_files = std::iter::once(&a.primary).chain(a.companions.iter());
        let has_raw = all_files.clone().any(|f| f.kind == FileKind::Raw);
        let has_jpeg = all_files.clone().any(|f| f.kind == FileKind::Jpeg);
        let camera = camera_name(exif.camera_make.as_deref(), exif.camera_model.as_deref());
        let status = match row {
            None => PhotoStatus::Pending,
            Some(r) if r.error.is_some() && r.metrics.is_none() => PhotoStatus::Error,
            Some(_) => PhotoStatus::Analyzed,
        };
        PhotoView {
            id: a.id.clone(),
            rel_path: a.rel_path.clone(),
            file_name: a
                .primary
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| a.rel_path.clone()),
            kind: a.primary.kind,
            has_raw,
            has_jpeg,
            size_bytes: all_files.map(|f| f.size).sum(),
            captured_at: exif.captured_at_ms,
            width: exif.width,
            height: exif.height,
            exif: ExifSummary {
                camera,
                lens: exif.lens.clone(),
                iso: exif.iso,
                shutter: exif.exposure_time_s.and_then(format_shutter),
                aperture: exif.f_number,
                focal_length: exif.focal_length_mm,
            },
            thumb_url: row.and_then(|r| r.thumb_path.as_ref()).map(|p| self.url_for(p)),
            status,
            error: row.and_then(|r| r.error.clone()),
            decision: final_decision(assessment.as_ref().map(|x| x.verdict), &mark),
            assessment,
            faces: row
                .map(|r| {
                    r.faces
                        .iter()
                        .map(|f| FaceView {
                            bbox: f.bbox,
                            confidence: f.confidence,
                            right_eye_open: f.right_eye_open,
                            left_eye_open: f.left_eye_open,
                            eyes_closed: f.eyes_closed(pipeline::EYE_OPEN_THRESHOLD),
                            sharpness: f.sharpness,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            mark,
            scene_id: st.scene_of.get(&a.id).copied(),
        }
    }

    // -- decisions ---------------------------------------------------------

    /// Re-run decisions with new settings (instant: cached measures).
    pub fn recompute(&self, project_id: &str, settings: CullSettings) -> Result<ProjectData> {
        let mut row = self.project_row(project_id)?;
        row.settings = settings.clone();
        self.store.lock().upsert_project(&row)?;
        let state = self.state(project_id)?;
        state.lock().decide(&settings);
        self.get_project(project_id)
    }

    pub fn set_marks(&self, project_id: &str, photo_ids: &[String], patch: MarkPatch) -> Result<Vec<PhotoView>> {
        let state = self.state(project_id)?;
        let mut st = state.lock();
        let mut changed = Vec::with_capacity(photo_ids.len());
        for id in photo_ids {
            if !st.assets.iter().any(|a| &a.id == id) {
                return Err(EngineError::PhotoNotFound(id.clone()));
            }
            let mark = st.marks.entry(id.clone()).or_default();
            patch.apply(mark);
            changed.push((id.clone(), *mark));
        }
        self.store.lock().save_marks(project_id, &changed, now_ms())?;
        let views = changed
            .iter()
            .filter_map(|(id, _)| st.assets.iter().find(|a| &a.id == id))
            .map(|a| self.photo_view(&st, a))
            .collect();
        Ok(views)
    }

    // -- analysis ----------------------------------------------------------

    pub fn is_running(&self, project_id: &str) -> bool {
        self.running.lock().contains_key(project_id)
    }

    /// A job the photographer started is under way: an analysis, an export,
    /// an upload to Pelli or the Maximum pack (download, preparation). The
    /// desktop shell installs an app update only once none is.
    pub fn busy(&self) -> bool {
        !self.running.lock().is_empty()
            || !self.uploads.lock().is_empty()
            || self.exports.load(Ordering::SeqCst) > 0
            || self.pack.is_busy()
    }

    /// Start (or resume) the analysis of a project in the background.
    /// Photos already analysed with the same file and models are skipped;
    /// those analysed for the first time are counted on the Pelli monthly
    /// quota (not in a [no-account build](no_account_build)).
    pub fn start_analysis(self: &Arc<Self>, project_id: &str, settings: CullSettings) -> Result<()> {
        let mut row = self.project_row(project_id)?;
        let cancel = {
            let mut running = self.running.lock();
            if running.contains_key(project_id) {
                return Err(EngineError::AlreadyRunning);
            }
            let flag = Arc::new(AtomicBool::new(false));
            running.insert(project_id.to_string(), flag.clone());
            flag
        };
        row.settings = settings;
        self.store.lock().upsert_project(&row)?;
        let engine = self.clone();
        let project_id = project_id.to_string();
        std::thread::Builder::new()
            .name(format!("analysis-{project_id}"))
            .spawn(move || {
                let _running = Running(&engine.running, &project_id);
                let job = AnalysisJob { engine: &engine, project: row, cancel, metered: !no_account_build() };
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job.run())).unwrap_or_else(|_| {
                        Err(EngineError::Remote { code: "internal".into(), message: "internal error".into() })
                    });
                if let Err(e) = result {
                    log::error!("analysis failed: {e}");
                    let mut payload = commands::error_json(&e);
                    payload["projectId"] = project_id.clone().into();
                    engine.events().emit("analysis-error", payload);
                }
            })
            .map_err(|e| EngineError::io(Path::new("thread"), e))?;
        Ok(())
    }

    pub fn cancel_analysis(&self, project_id: &str) {
        if let Some(flag) = self.running.lock().get(project_id) {
            flag.store(true, Ordering::Relaxed);
        }
    }

    /// Blocking variant for the CLI and tests (developer tools): never
    /// counts photos on the Pelli quota, so it needs no account.
    pub fn analyze_blocking(self: &Arc<Self>, project_id: &str, settings: CullSettings) -> Result<AnalysisSummary> {
        let mut row = self.project_row(project_id)?;
        row.settings = settings;
        self.store.lock().upsert_project(&row)?;
        let job = AnalysisJob { engine: self, project: row, cancel: Arc::new(AtomicBool::new(false)), metered: false };
        job.run()
    }

    // -- previews & search -------------------------------------------------

    /// Path of a full-quality JPEG of the photo for the loupe: the original
    /// JPEG when there is one, else the RAW's embedded JPEG (extracted once
    /// into the cache).
    pub fn large_preview_path(&self, project_id: &str, photo_id: &str) -> Result<PathBuf> {
        let state = self.state(project_id)?;
        let asset = state
            .lock()
            .assets
            .iter()
            .find(|a| a.id == photo_id)
            .cloned()
            .ok_or_else(|| EngineError::PhotoNotFound(photo_id.to_string()))?;
        if asset.primary.kind == FileKind::Jpeg {
            return Ok(asset.primary.path.clone());
        }
        let cached = self.project_cache_dir(project_id).join(format!("{photo_id}.f.jpg"));
        if cached.exists() {
            return Ok(cached);
        }
        let bytes = match cull_decode::best_jpeg_bytes(&asset.primary) {
            Ok(b) => b,
            Err(e) => {
                // RAW without usable preview: fall back to its JPEG twin.
                match asset.companions.iter().find(|c| c.kind == FileKind::Jpeg) {
                    Some(twin) => return Ok(twin.path.clone()),
                    None => return Err(e.into()),
                }
            }
        };
        pipeline::write_atomic(&cached, &bytes)?;
        Ok(cached)
    }

    pub fn large_preview(&self, project_id: &str, photo_id: &str) -> Result<String> {
        Ok(self.url_for(&self.large_preview_path(project_id, photo_id)?))
    }

    /// Natural-language search over CLIP embeddings, best first. The query
    /// is embedded by the text tower of the CLIP model that analysed the
    /// project (ViT-B/32 in Standard precision, ViT-L/14 in Maximum: their
    /// embeddings are not comparable), and only photos analysed by that
    /// model are ranked.
    pub fn search(&self, project_id: &str, query: &str) -> Result<Vec<SearchHit>> {
        let prompt = query::to_clip_prompt(query);
        let maximum = self.project_row(project_id)?.precision == Some(cull_types::Precision::Maximum);
        let q = if maximum {
            self.maximum_text().ok_or(EngineError::SearchUnavailable)?.embed_text(&prompt)?
        } else {
            let models = self.models().ok_or(EngineError::SearchUnavailable)?;
            if !models.capabilities().text_search {
                return Err(EngineError::SearchUnavailable);
            }
            models.embed_text(&prompt)?
        };
        let state = self.state(project_id)?;
        let st = state.lock();
        let mut hits: Vec<SearchHit> = st
            .assets
            .iter()
            .filter_map(|a| {
                let row = st.analyses.get(&a.id)?;
                if cull_ml::is_maximum_models_id(&row.models) != maximum {
                    return None;
                }
                let e = if maximum { row.search_embedding.as_ref()? } else { row.embedding.as_ref()? };
                if e.len() != q.len() {
                    return None;
                }
                let score = e.iter().zip(&q).map(|(x, y)| x * y).sum::<f32>();
                Some(SearchHit { photo_id: a.id.clone(), score })
            })
            .collect();
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        // CLIP similarities of matching photos sit ~0.05 above the rest:
        // keep a band below the best hit instead of an absolute threshold.
        if let Some(best) = hits.first().map(|h| h.score) {
            hits.retain(|h| h.score >= best - 0.06);
        }
        hits.truncate(500);
        Ok(hits)
    }

    // -- cache -------------------------------------------------------------

    pub fn cache_size(&self) -> u64 {
        fn walk(p: &Path) -> u64 {
            let Ok(rd) = std::fs::read_dir(p) else { return 0 };
            rd.flatten()
                .map(|e| match e.file_type() {
                    Ok(t) if t.is_dir() => walk(&e.path()),
                    Ok(_) => e.metadata().map(|m| m.len()).unwrap_or(0),
                    Err(_) => 0,
                })
                .sum()
        }
        walk(&self.config.cache_dir)
    }

    /// Delete every cached preview. Analyses are kept; thumbnails are
    /// regenerated by the next analysis run (cheap: decode only).
    pub fn clear_cache(&self) -> Result<()> {
        for running in self.running.lock().values() {
            running.store(true, Ordering::Relaxed);
        }
        let dir = &self.config.cache_dir;
        if dir.exists() {
            for e in std::fs::read_dir(dir).map_err(|e| EngineError::io(dir, e))?.flatten() {
                let p = e.path();
                let r = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
                r.map_err(|e| EngineError::io(&p, e))?;
            }
        }
        // Thumbnails are gone: forget their paths so the UI shows placeholders.
        for state in self.states.lock().values() {
            for row in state.lock().analyses.values_mut() {
                row.thumb_path = None;
            }
        }
        Ok(())
    }
}
