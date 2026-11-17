//! Sending the selection to a Pelli gallery.
//!
//! Gallery and folders are created through the Pelli API; files go straight
//! to Pelli's upload service with a short-lived gallery upload token, the
//! same path the Lightroom plugin uses (`docs/pelli-api.md`).
//!
//! What a photo sends is always a JPEG, never a RAW: the camera's JPEG twin
//! of a RAW + JPEG pair, the file itself for a JPEG, and for a RAW alone the
//! largest JPEG preview its camera embedded in it (full size on most recent
//! bodies, smaller on some: [`Engine::upload_check`] says how big).

use crate::commands::{UploadRequest, UploadWhich};
use crate::pelli::{GallerySummary, PelliClient};
use crate::uploader::UploadFile;
use crate::{Engine, EngineError, Result};
use cull_types::{final_decision, FileKind, FinalDecision, PhotoAsset, Scene};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// Deterministic UUID-shaped id (idempotent creations on retry).
pub(crate) fn request_id(parts: &[&str]) -> String {
    let h = sha1_smol::Sha1::from(parts.join("\u{1f}")).digest().bytes();
    format!(
        "{:08x}-{:04x}-5{:03x}-{:04x}-{:012x}",
        u32::from_be_bytes([h[0], h[1], h[2], h[3]]),
        u16::from_be_bytes([h[4], h[5]]),
        u16::from_be_bytes([h[6], h[7]]) & 0x0fff,
        (u16::from_be_bytes([h[8], h[9]]) & 0x3fff) | 0x8000,
        u64::from_be_bytes([0, 0, h[10], h[11], h[12], h[13], h[14], h[15]]),
    )
}

/// The camera JPEG of a photo, when it has one (itself, or a RAW's twin).
fn jpeg_twin(asset: &PhotoAsset) -> Option<&cull_types::SourceFile> {
    std::iter::once(&asset.primary).chain(asset.companions.iter()).find(|f| f.kind == FileKind::Jpeg)
}

/// A RAW without a JPEG twin: the gallery gets its embedded preview.
fn raw_only(asset: &PhotoAsset) -> bool {
    asset.primary.kind == FileKind::Raw && jpeg_twin(asset).is_none()
}

/// What is sent for a photo: the camera JPEG twin when there is one, else
/// the file's best full-quality JPEG (the largest preview embedded in a
/// RAW). The JPEG is read straight into the request body.
pub(crate) fn upload_file(asset: &PhotoAsset) -> Result<UploadFile> {
    let twin = jpeg_twin(asset);
    let file = twin.unwrap_or(&asset.primary);
    let stem = file.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| asset.stem.clone());
    UploadFile::build(&format!("{stem}.jpg"), |body| match twin {
        Some(f) => std::fs::File::open(&f.path)
            .and_then(|mut fh| fh.read_to_end(body))
            .map(|_| ())
            .map_err(|e| EngineError::io(&f.path, e)),
        None => {
            body.extend_from_slice(&cull_decode::best_jpeg_bytes(&asset.primary)?);
            Ok(())
        }
    })
}

/// What the photographer should know before sending (`upload_check`).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UploadCheck {
    /// Photos the upload would send.
    pub photos: u32,
    /// Of those, RAW files without a JPEG twin.
    pub raw_only: u32,
    /// Long edge, in pixels, of the smallest preview embedded in those RAWs
    /// (among a sample): what the gallery gets for them. `None` without such
    /// a photo, or when none could be read.
    pub preview_long_edge: Option<u32>,
}

/// The photos an upload sends, each with its moment, and the moments of the
/// shoot.
type Selection = (Vec<(PhotoAsset, Option<u32>)>, Vec<Scene>);

/// RAW files whose embedded previews [`Engine::upload_check`] reads (spread
/// over the selection, so a second camera body shows up).
const CHECK_SAMPLE: usize = 16;

impl Engine {
    pub fn cancel_upload(&self, project_id: &str) {
        if let Some(flag) = self.uploads.lock().get(project_id) {
            flag.store(true, Ordering::Relaxed);
        }
    }

    pub fn start_upload(self: &Arc<Self>, project_id: &str, request: UploadRequest) -> Result<()> {
        let client = self.signed_in_client()?;
        let cancel = {
            let mut uploads = self.uploads.lock();
            if uploads.contains_key(project_id) {
                return Err(EngineError::AlreadyRunning);
            }
            let flag = Arc::new(AtomicBool::new(false));
            uploads.insert(project_id.to_string(), flag.clone());
            flag
        };
        let engine = self.clone();
        let pid = project_id.to_string();
        std::thread::Builder::new()
            .name(format!("upload-{pid}"))
            .spawn(move || {
                let _running = crate::Running(&engine.uploads, &pid);
                let _awake = crate::system::KeepAwake::new("Pelli Instant: sending photos to Pelli");
                if let Err(e) = engine.run_upload(&client, &pid, &request, &cancel) {
                    let mut payload = crate::commands::error_json(&e);
                    payload["projectId"] = pid.clone().into();
                    engine.events().emit("upload-error", payload);
                }
            })
            .map_err(|e| EngineError::io(std::path::Path::new("thread"), e))?;
        Ok(())
    }

    /// What an upload of `which` sends (by final decision: the
    /// photographer's flag first, else the AI's verdict).
    fn upload_selection(&self, project_id: &str, which: UploadWhich) -> Result<Selection> {
        let state = self.state(project_id)?;
        let st = state.lock();
        let photos = st
            .assets
            .iter()
            .filter(|a| {
                let mark = st.marks.get(&a.id).copied().unwrap_or_default();
                let d = final_decision(st.assessments.get(&a.id).map(|x| x.verdict), &mark);
                d == FinalDecision::Selected
                    || (which == UploadWhich::SelectedAndToReview && d == FinalDecision::ToReview)
            })
            .map(|a| (a.clone(), st.scene_of.get(&a.id).copied()))
            .collect();
        Ok((photos, st.scenes.clone()))
    }

    /// Before sending: how many photos would go, how many are RAW files
    /// alone, and how big the previews embedded in those are. Reads a few
    /// KiB of a few files; nothing leaves the computer.
    pub fn upload_check(&self, project_id: &str, which: UploadWhich) -> Result<UploadCheck> {
        let (photos, _) = self.upload_selection(project_id, which)?;
        let raws: Vec<&PhotoAsset> = photos.iter().map(|(a, _)| a).filter(|a| raw_only(a)).collect();
        let n = raws.len().min(CHECK_SAMPLE);
        let preview_long_edge = (0..n)
            .filter_map(|i| {
                let asset = raws[i * raws.len() / n];
                let probe = cull_decode::probe_previews(&asset.primary.path, 0).ok()?;
                // The preview sent is the largest one (`best_jpeg_bytes`).
                let largest = probe.previews.iter().max_by_key(|p| u64::from(p.width) * u64::from(p.height))?;
                Some(largest.width.max(largest.height))
            })
            .min();
        Ok(UploadCheck { photos: photos.len() as u32, raw_only: raws.len() as u32, preview_long_edge })
    }

    fn run_upload(
        &self,
        client: &PelliClient,
        project_id: &str,
        request: &UploadRequest,
        cancel: &AtomicBool,
    ) -> Result<()> {
        let project = self.project_row(project_id)?;
        let (photos, scenes) = self.upload_selection(project_id, request.which)?;
        if photos.is_empty() {
            return Err(EngineError::Remote { code: "nothingToUpload".into(), message: "no photo selected".into() });
        }

        // Gallery. A new gallery uses a request id derived from the project
        // and title: sending again lands in the same gallery (no duplicate
        // galleries on retry), where only what is new is uploaded.
        let gallery: GallerySummary =
            match (&request.gallery_id, &request.new_gallery) {
                (Some(id), _) => client.list_galleries()?.into_iter().find(|g| &g.id == id).ok_or_else(|| {
                    EngineError::Remote { code: "gallery_not_found".into(), message: "gallery".into() }
                })?,
                (None, _) => {
                    let (title, kind) = match &request.new_gallery {
                        Some(new) => (new.title.trim(), if new.kind == "delivery" { "delivery" } else { "selection" }),
                        None => (project.name.as_str(), "selection"),
                    };
                    match client.create_gallery(&request_id(&[project_id, title, kind]), title, kind) {
                        // That gallery was deleted in Pelli since: create a fresh one.
                        Err(e) if e.code() == "request_conflict" => client.create_gallery(
                            &request_id(&[project_id, title, kind, &crate::now_ms().to_string()]),
                            title,
                            kind,
                        )?,
                        other => other?,
                    }
                }
            };

        // Skip what this gallery already received (same file), so sending
        // again after a few more picks only uploads the new ones.
        let already = self.store.lock().uploaded(project_id, &gallery.id)?;
        let (photos, skipped): (Vec<_>, Vec<_>) =
            photos.into_iter().partition(|(a, _)| already.get(&a.id) != Some(&(a.primary.size, a.primary.modified_ms)));
        let skipped = skipped.len() as u32;

        // One folder per scene (only when there is more than one scene).
        let mut folders: HashMap<u32, String> = HashMap::new();
        if request.folder_per_scene && scenes.len() > 1 {
            let lang = self.language();
            let days: std::collections::HashSet<i64> =
                scenes.iter().filter_map(|s| s.start_ms).map(|ms| ms.div_euclid(86_400_000)).collect();
            let multi_day = days.len() > 1;
            for (i, scene) in scenes.iter().enumerate() {
                if !photos.iter().any(|(_, s)| *s == Some(scene.id)) {
                    continue;
                }
                let name = scene_folder_name(&lang, i + 1, scene.start_ms, multi_day);
                let folder = match client.create_folder(&request_id(&[&gallery.id, &name]), &gallery.id, &name) {
                    // A folder of that name was made in Pelli by hand: use a
                    // distinct one rather than mixing the photos in.
                    Err(e) if e.code() == "folder_exists" => {
                        let other = format!("{name} (Pelli Instant)");
                        client.create_folder(&request_id(&[&gallery.id, &other]), &gallery.id, &other)?
                    }
                    other => other?,
                };
                folders.insert(scene.id, folder.id);
            }
        }

        let url =
            gallery.url.clone().unwrap_or_else(|| format!("{}/dashboard/gallery/{}", client.base_url(), gallery.slug));
        if photos.is_empty() {
            self.events().emit(
                "upload-done",
                json!({ "projectId": project_id, "galleryId": gallery.id, "galleryUrl": url,
                        "uploaded": 0, "failed": 0, "skipped": skipped }),
            );
            return Ok(());
        }

        let target = crate::uploader::UploadTarget::fetch(client, &gallery.id)?;
        // The upload service does not enforce storage: check it here.
        if let Some(free) = target.storage_free {
            let needed: u64 =
                photos.iter().map(|(a, _)| jpeg_twin(a).map(|f| f.size).unwrap_or(a.primary.size / 3)).sum();
            if needed > free {
                return Err(EngineError::Remote {
                    code: "storage_full".into(),
                    message: format!("{needed} bytes needed, {free} available"),
                });
            }
        }

        let total = photos.len() as u32;
        let next = AtomicUsize::new(0);
        let done = AtomicU32::new(0);
        let failed = AtomicU32::new(0);
        let bytes_sent = AtomicU64::new(0);
        let fatal: Mutex<Option<EngineError>> = Mutex::new(None);
        std::thread::scope(|scope| {
            for _ in 0..target.max_concurrency {
                scope.spawn(|| loop {
                    if cancel.load(Ordering::Relaxed) || fatal.lock().is_some() {
                        return;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some((asset, scene)) = photos.get(i) else { return };
                    self.events().emit(
                        "upload-progress",
                        json!({ "projectId": project_id, "galleryId": gallery.id,
                                "done": done.load(Ordering::Relaxed), "total": total,
                                "bytesSent": bytes_sent.load(Ordering::Relaxed), "current": asset.rel_path }),
                    );
                    let folder = scene.and_then(|s| folders.get(&s)).map(String::as_str);
                    let result = upload_file(asset).and_then(|file| {
                        match target.upload(client, folder, &file) {
                            // Folder deleted meanwhile: the gallery root still works.
                            Err(e) if e.code() == "folder_not_found" => target.upload(client, None, &file),
                            other => other,
                        }
                        .map(|_| file.size)
                    });
                    match result {
                        Ok(n) => {
                            done.fetch_add(1, Ordering::Relaxed);
                            bytes_sent.fetch_add(n, Ordering::Relaxed);
                            let fingerprint = (asset.primary.size, asset.primary.modified_ms);
                            if let Err(e) = self.store.lock().mark_uploaded(
                                project_id,
                                &gallery.id,
                                &asset.id,
                                fingerprint,
                                crate::now_ms(),
                            ) {
                                log::warn!("upload of {} not remembered: {e}", asset.rel_path);
                            }
                        }
                        Err(e) => {
                            log::warn!("upload of {} failed: {e}", asset.rel_path);
                            failed.fetch_add(1, Ordering::Relaxed);
                            if matches!(
                                e.code(),
                                "unauthorized"
                                    | "offline"
                                    | "upload_forbidden"
                                    | "gallery_not_found"
                                    | "subscription_required"
                            ) {
                                *fatal.lock() = Some(e);
                            }
                        }
                    }
                });
            }
        });
        if let Some(e) = fatal.into_inner() {
            return Err(e);
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(EngineError::Cancelled);
        }
        let (done, failed) = (done.into_inner(), failed.into_inner());
        self.events().emit(
            "upload-progress",
            json!({ "projectId": project_id, "galleryId": gallery.id, "done": done, "total": total,
                    "bytesSent": bytes_sent.into_inner(), "current": null }),
        );
        self.events().emit(
            "upload-done",
            json!({ "projectId": project_id, "galleryId": gallery.id, "galleryUrl": url,
                    "uploaded": done, "failed": failed, "skipped": skipped }),
        );
        Ok(())
    }
}

/// Name of the Pelli folder of a moment, in the app language (English for
/// another one): "Moment 2 · 14:05", "Moment 2 · Jun 14 14:05" when the
/// shoot spans several days, "Undated" for photos without capture time.
/// Never a `/`: folder names become paths in the gallery ZIPs.
fn scene_folder_name(lang: &str, index: usize, start_ms: Option<i64>, multi_day: bool) -> String {
    let (word, undated, months): (&str, &str, [&str; 12]) = match lang {
        "fr" => (
            "Moment",
            "Sans date",
            ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."],
        ),
        "es" => (
            "Momento",
            "Sin fecha",
            ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sept", "oct", "nov", "dic"],
        ),
        "de" => (
            "Moment",
            "Ohne Datum",
            ["Jan.", "Feb.", "März", "Apr.", "Mai", "Juni", "Juli", "Aug.", "Sept.", "Okt.", "Nov.", "Dez."],
        ),
        "it" => (
            "Momento",
            "Senza data",
            ["gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic"],
        ),
        "pt" => (
            "Momento",
            "Sem data",
            ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"],
        ),
        "nl" => (
            "Moment",
            "Zonder datum",
            ["jan", "feb", "mrt", "apr", "mei", "jun", "jul", "aug", "sep", "okt", "nov", "dec"],
        ),
        "pl" => {
            ("Moment", "Bez daty", ["sty", "lut", "mar", "kwi", "maj", "cze", "lip", "sie", "wrz", "paź", "lis", "gru"])
        }
        "sv" => (
            "Ögonblick",
            "Utan datum",
            ["jan", "feb", "mars", "apr", "maj", "juni", "juli", "aug", "sep", "okt", "nov", "dec"],
        ),
        _ => {
            ("Moment", "Undated", ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"])
        }
    };
    let Some(ms) = start_ms else { return undated.to_string() };
    let secs = ms.div_euclid(1000);
    let (h, m) = ((secs / 3600).rem_euclid(24), (secs / 60).rem_euclid(60));
    let time = if lang == "fr" { format!("{h:02}h{m:02}") } else { format!("{h:02}:{m:02}") };
    if !multi_day {
        return format!("{word} {index} · {time}");
    }
    let (_, month, day) = civil_from_days(secs.div_euclid(86_400));
    let month = months[(month as usize).clamp(1, 12) - 1];
    let date = match lang {
        "de" => format!("{day}. {month}"),
        "fr" | "es" | "it" | "pt" | "nl" | "pl" | "sv" => format!("{day} {month}"),
        _ => format!("{month} {day}"),
    };
    format!("{word} {index} · {date} {time}")
}

/// (year, month, day) of a day count since 1970-01-01 (H. Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::NewGallery;
    use crate::testing::{fake_pelli, test_engine, FakePelli, Recorder, Request};
    use crate::view::MarkPatch;
    use cull_types::{CullSettings, Flag};
    use image::{Rgb, RgbImage};
    use serde_json::Value;
    use std::path::Path;
    use std::time::{Duration, Instant};

    /// Textured, sharp frame (seeded so frames differ).
    fn frame(seed: u32, w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_fn(w, h, |x, y| {
            let v = ((x / 7 + y / 5 + seed) % 2) * 160 + ((x * 31 + y * 17 + seed * 13) % 60);
            Rgb([v as u8, (v / 2 + 40) as u8, (255 - v) as u8])
        });
        cull_decode::encode_jpeg(&img, 88)
    }

    /// A TIFF-based RAW whose only image is the JPEG preview its camera
    /// embedded (IFD0 `JPEGInterchangeFormat`), like a Sony A7 III's 1616 px.
    fn raw_with_preview(preview: &[u8]) -> Vec<u8> {
        let mut out = b"II*\0".to_vec();
        out.extend(8u32.to_le_bytes());
        out.extend(2u16.to_le_bytes());
        let data_at = 8 + 2 + 2 * 12 + 4;
        for (tag, value) in [(0x0201u16, data_at), (0x0202, preview.len() as u32)] {
            out.extend(tag.to_le_bytes());
            out.extend(4u16.to_le_bytes());
            out.extend(1u32.to_le_bytes());
            out.extend(value.to_le_bytes());
        }
        out.extend(0u32.to_le_bytes());
        out.extend_from_slice(preview);
        out
    }

    /// Three JPEGs, a RAW + JPEG pair (the RAW unreadable: never sent) and a
    /// RAW alone, analysed; every photo picked but `IMG_0003.jpg`, rejected.
    fn shoot(dir: &Path, events: Arc<Recorder>) -> (Arc<Engine>, String, std::path::PathBuf) {
        let root = dir.join("Mariage");
        std::fs::create_dir_all(&root).unwrap();
        for i in 1..=3 {
            std::fs::write(root.join(format!("IMG_000{i}.jpg")), frame(i * 7, 900, 600)).unwrap();
        }
        std::fs::write(root.join("IMG_0200.dng"), b"II*\0not a real raw").unwrap();
        std::fs::write(root.join("IMG_0200.jpg"), frame(50, 900, 600)).unwrap();
        std::fs::write(root.join("IMG_0300.arw"), raw_with_preview(&frame(80, 1616, 1080))).unwrap();
        let engine = test_engine(dir, events);
        let project = engine.open_folder(&root, true).unwrap();
        engine.analyze_blocking(&project.id, CullSettings::default()).unwrap();
        let ids: HashMap<String, String> =
            engine.get_project(&project.id).unwrap().photos.into_iter().map(|p| (p.file_name, p.id)).collect();
        let flag = |flag| MarkPatch { flag: Some(flag), rating: None, label: None };
        let picked: Vec<String> =
            ["IMG_0001.jpg", "IMG_0002.jpg", "IMG_0200.dng", "IMG_0300.arw"].iter().map(|n| ids[*n].clone()).collect();
        engine.set_marks(&project.id, &picked, flag(Flag::Pick)).unwrap();
        engine.set_marks(&project.id, std::slice::from_ref(&ids["IMG_0003.jpg"]), flag(Flag::Reject)).unwrap();
        // Signed in to Pelli (the token in the library, as in debug builds).
        engine.kv_set("pelli.token", "access-token").unwrap();
        (engine, project.id, root)
    }

    /// A fake Pelli that creates galleries and folders and accepts every
    /// photo; `photo` answers the upload service.
    fn pelli(photo: impl Fn(&Request) -> (u16, Value) + Send + Sync + 'static) -> FakePelli {
        let base = Arc::new(Mutex::new(String::new()));
        let origin = base.clone();
        let fake = fake_pelli(move |req| match req.path.split('?').next().unwrap_or_default() {
            "/api/cull/galleries/create" => (
                200,
                json!({ "gallery": { "id": "g1", "title": req.body["title"], "slug": "mariage", "status": "draft",
                                     "imageCount": 0, "coverUrl": null, "createdAt": 1,
                                     "url": "https://app.pelli.io/dashboard/gallery/mariage" },
                        "password": null }),
            ),
            "/api/cull/folders/create" => {
                (200, json!({ "folder": { "id": format!("f-{}", req.body["name"]), "name": req.body["name"] } }))
            }
            "/api/cull/upload-token" => (
                200,
                json!({ "uploadToken": "upload-token", "uploadUrl": format!("{}/api/photo", origin.lock()),
                        "expiresAt": crate::now_ms() + 3_600_000, "maxFileSize": 104_857_600, "maxConcurrency": 1,
                        "storage": { "usedBytes": 0, "limitBytes": 1u64 << 40 } }),
            ),
            "/api/photo" => photo(req),
            _ => (404, json!({ "code": "not_found", "message": req.path })),
        });
        *base.lock() = fake.url.clone();
        fake
    }

    fn new_gallery(title: &str) -> UploadRequest {
        UploadRequest {
            gallery_id: None,
            new_gallery: Some(NewGallery { title: title.into(), kind: "selection".into() }),
            which: UploadWhich::Selected,
            folder_per_scene: true,
        }
    }

    /// The `count`-th `name` event, once emitted.
    fn wait_for(events: &Recorder, name: &str, count: usize) -> Value {
        let start = Instant::now();
        loop {
            if let Some(v) = events.named(name).get(count - 1) {
                return v.clone();
            }
            assert!(
                start.elapsed() < Duration::from_secs(60),
                "no {name} #{count}: {:?}",
                events.named("upload-error")
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Until the upload thread is gone (it ends just after its last event).
    fn wait_idle(engine: &Engine) {
        let start = Instant::now();
        while engine.busy() {
            assert!(start.elapsed() < Duration::from_secs(10), "still busy");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The photos the upload service received: file name (query) and bytes.
    fn received(pelli: &FakePelli) -> Vec<(String, Vec<u8>)> {
        pelli
            .requests()
            .into_iter()
            .filter(|r| r.path.starts_with("/api/photo?"))
            .map(|r| {
                let name = r.path.split(['?', '&']).find_map(|p| p.strip_prefix("name=")).unwrap_or_default();
                let name = percent_encoding::percent_decode_str(name).decode_utf8_lossy().into_owned();
                let start = r.raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
                let end = r.raw.windows(4).rposition(|w| w == b"\r\n--").unwrap();
                (name, r.raw[start..end].to_vec())
            })
            .collect()
    }

    #[test]
    fn sends_the_selection_once_then_only_what_is_new() {
        let tmp = tempfile::tempdir().unwrap();
        let events = Arc::new(Recorder::default());
        let (engine, pid, root) = shoot(tmp.path(), events.clone());
        let pelli = pelli(|req| {
            assert_eq!(req.authorization.as_deref(), Some("upload-token"), "the raw upload token, no Bearer");
            (200, json!({ "success": true, "imageId": "i" }))
        });
        *engine.api_url.write() = pelli.url.clone();

        // What the photographer is told before sending: one RAW alone, whose
        // camera embedded a 1616 px preview. Nothing asked to Pelli.
        let check = engine.upload_check(&pid, UploadWhich::Selected).unwrap();
        assert_eq!(check, UploadCheck { photos: 4, raw_only: 1, preview_long_edge: Some(1616) });
        assert!(pelli.requests().is_empty());

        engine.start_upload(&pid, new_gallery("Mariage")).unwrap();
        let done = wait_for(&events, "upload-done", 1);
        assert_eq!(
            done,
            json!({ "projectId": pid, "galleryId": "g1", "galleryUrl": "https://app.pelli.io/dashboard/gallery/mariage",
                    "uploaded": 4, "failed": 0, "skipped": 0 })
        );
        let creates: Vec<Request> =
            pelli.requests().into_iter().filter(|r| r.path == "/api/cull/galleries/create").collect();
        assert_eq!(creates.len(), 1);
        let request_id = request_id(&[&pid, "Mariage", "selection"]);
        assert_eq!(creates[0].body, json!({ "requestId": request_id, "title": "Mariage", "type": "selection" }));
        assert_eq!(creates[0].authorization.as_deref(), Some("Bearer access-token"));

        // JPEGs only: the twin of the pair, the RAW alone's embedded preview.
        let mut sent = received(&pelli);
        sent.sort();
        let names: Vec<&str> = sent.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["IMG_0001.jpg", "IMG_0002.jpg", "IMG_0200.jpg", "IMG_0300.jpg"]);
        for (name, bytes) in &sent {
            assert!(bytes.starts_with(&[0xFF, 0xD8]) && bytes.ends_with(&[0xFF, 0xD9]), "{name} is a JPEG");
        }
        assert_eq!(sent[2].1, std::fs::read(root.join("IMG_0200.jpg")).unwrap(), "the camera JPEG, as it is");
        let preview = cull_decode::decode_jpeg(&sent[3].1, 4000).unwrap();
        assert_eq!(preview.dimensions(), (1616, 1080));
        wait_idle(&engine);

        // Sending again lands in the same gallery (same request id) and
        // sends nothing it already has.
        engine.start_upload(&pid, new_gallery("Mariage")).unwrap();
        let again = wait_for(&events, "upload-done", 2);
        assert_eq!((again["uploaded"].as_u64(), again["skipped"].as_u64()), (Some(0), Some(4)));
        let creates: Vec<Request> =
            pelli.requests().into_iter().filter(|r| r.path == "/api/cull/galleries/create").collect();
        assert_eq!(creates.len(), 2);
        assert_eq!(creates[1].body["requestId"], request_id);
        assert_eq!(received(&pelli).len(), 4);
        wait_idle(&engine);

        // One more pick: only that one is sent.
        let id =
            engine.get_project(&pid).unwrap().photos.into_iter().find(|p| p.file_name == "IMG_0003.jpg").unwrap().id;
        engine.set_marks(&pid, &[id], MarkPatch { flag: Some(Flag::Pick), rating: None, label: None }).unwrap();
        engine.start_upload(&pid, new_gallery("Mariage")).unwrap();
        let third = wait_for(&events, "upload-done", 3);
        assert_eq!((third["uploaded"].as_u64(), third["skipped"].as_u64()), (Some(1), Some(4)));
        let sent = received(&pelli);
        assert_eq!((sent.len(), sent[4].0.as_str()), (5, "IMG_0003.jpg"));
        assert!(events.named("upload-error").is_empty());
    }

    #[test]
    fn cancelling_stops_after_the_photo_under_way() {
        let tmp = tempfile::tempdir().unwrap();
        let events = Arc::new(Recorder::default());
        let (engine, pid, _) = shoot(tmp.path(), events.clone());
        let started = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(AtomicBool::new(false));
        let (s, r) = (started.clone(), release.clone());
        let pelli = pelli(move |_| {
            s.fetch_add(1, Ordering::SeqCst);
            let start = Instant::now();
            while !r.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(20) {
                std::thread::sleep(Duration::from_millis(5));
            }
            (200, json!({ "success": true, "imageId": "i" }))
        });
        *engine.api_url.write() = pelli.url.clone();

        engine.start_upload(&pid, new_gallery("Mariage")).unwrap();
        let start = Instant::now();
        while started.load(Ordering::SeqCst) == 0 {
            assert!(start.elapsed() < Duration::from_secs(30), "no photo sent");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(engine.start_upload(&pid, new_gallery("Mariage")).unwrap_err().code(), "alreadyRunning");
        engine.cancel_upload(&pid);
        release.store(true, Ordering::SeqCst);
        let error = wait_for(&events, "upload-error", 1);
        assert_eq!((error["code"].as_str(), error["projectId"].as_str()), (Some("cancelled"), Some(pid.as_str())));
        assert!(events.named("upload-done").is_empty());
        wait_idle(&engine);
        assert_eq!(started.load(Ordering::SeqCst), 1, "nothing sent after the cancel");

        // The photo sent before the cancel is remembered: sending again
        // finishes the job.
        engine.start_upload(&pid, new_gallery("Mariage")).unwrap();
        let done = wait_for(&events, "upload-done", 1);
        assert_eq!((done["uploaded"].as_u64(), done["skipped"].as_u64()), (Some(3), Some(1)));
        assert_eq!(started.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn behind_the_paywall_or_signed_out_nothing_is_sent() {
        let tmp = tempfile::tempdir().unwrap();
        let events = Arc::new(Recorder::default());
        let (engine, pid, _) = shoot(tmp.path(), events.clone());
        let pelli = fake_pelli(|req| match req.path.as_str() {
            "/api/cull/galleries/create" => (403, json!({ "error": "subscription_required", "message": "trial over" })),
            _ => (200, json!({ "success": true })),
        });
        *engine.api_url.write() = pelli.url.clone();
        engine.start_upload(&pid, new_gallery("Mariage")).unwrap();
        let error = wait_for(&events, "upload-error", 1);
        assert_eq!(error["code"], "subscription_required");
        assert_eq!(pelli.requests().len(), 1, "stopped at the gallery");
        wait_idle(&engine);

        engine.auth_logout().unwrap();
        assert_eq!(engine.start_upload(&pid, new_gallery("Mariage")).unwrap_err().code(), "unauthorized");
        assert_eq!(pelli.requests().len(), 1);
    }

    #[test]
    fn request_ids_are_stable_uuids() {
        let a = request_id(&["p", "Mariage"]);
        assert_eq!(a, request_id(&["p", "Mariage"]));
        assert_ne!(a, request_id(&["p", "Mariage 2"]));
        assert_eq!(a.len(), 36);
        assert_eq!(&a[14..15], "5");
    }

    #[test]
    fn scene_names() {
        let t = (14 * 3600 + 5 * 60) * 1000;
        assert_eq!(scene_folder_name("fr", 2, Some(t), false), "Moment 2 · 14h05");
        assert_eq!(scene_folder_name("en", 2, Some(t), false), "Moment 2 · 14:05");
        assert_eq!(scene_folder_name("fr", 4, None, false), "Sans date");
        assert_eq!(scene_folder_name("es", 4, None, true), "Sin fecha");
        // 2026-06-14 14:05 UTC
        let june = 1_781_445_900_000;
        assert_eq!(scene_folder_name("fr", 1, Some(june), true), "Moment 1 · 14 juin 14h05");
        assert_eq!(scene_folder_name("en", 1, Some(june), true), "Moment 1 · Jun 14 14:05");
        assert_eq!(scene_folder_name("de", 1, Some(june), true), "Moment 1 · 14. Juni 14:05");
        assert_eq!(scene_folder_name("pt", 1, Some(june), true), "Momento 1 · 14 jun 14:05");
        assert_eq!(scene_folder_name("nl", 3, Some(t), false), "Moment 3 · 14:05");
        assert_eq!(scene_folder_name("nl", 1, Some(june), true), "Moment 1 · 14 jun 14:05");
        assert_eq!(scene_folder_name("pl", 1, Some(june), true), "Moment 1 · 14 cze 14:05");
        assert_eq!(scene_folder_name("pl", 1, None, true), "Bez daty");
        assert_eq!(scene_folder_name("sv", 1, Some(june), true), "Ögonblick 1 · 14 juni 14:05");
        assert_eq!(scene_folder_name("pt", 2, None, false), "Sem data");
        // Another language: English, never French.
        assert_eq!(scene_folder_name("ja", 2, Some(t), false), "Moment 2 · 14:05");
        assert_eq!(scene_folder_name("", 1, None, false), "Undated");
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(20_513), (2026, 3, 1));
    }
}
