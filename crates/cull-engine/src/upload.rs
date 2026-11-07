//! Sending the selection to a Pelli gallery.
//!
//! Gallery and folders are created through the Pelli API; files go straight
//! to Pelli's upload service with a short-lived gallery upload token, the
//! same path the Lightroom plugin uses (`docs/pelli-api.md`).

use crate::commands::{UploadRequest, UploadWhich};
use crate::pelli::{GallerySummary, PelliClient};
use crate::{Engine, EngineError, Result};
use cull_types::{final_decision, FileKind, FinalDecision, PhotoAsset};
use parking_lot::Mutex;
use serde_json::json;
use std::collections::HashMap;
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

/// Bytes sent for a photo: the camera JPEG twin when there is one, else the
/// file's best full-quality JPEG (embedded preview of a RAW).
pub(crate) fn upload_bytes(asset: &PhotoAsset) -> Result<(String, Vec<u8>)> {
    let twin = std::iter::once(&asset.primary).chain(asset.companions.iter()).find(|f| f.kind == FileKind::Jpeg);
    let (file, bytes) = match twin {
        Some(f) => (f, std::fs::read(&f.path).map_err(|e| EngineError::io(&f.path, e))?),
        None => (&asset.primary, cull_decode::best_jpeg_bytes(&asset.primary)?),
    };
    let stem = file.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| asset.stem.clone());
    Ok((format!("{stem}.jpg"), bytes))
}

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

    fn run_upload(
        &self,
        client: &PelliClient,
        project_id: &str,
        request: &UploadRequest,
        cancel: &AtomicBool,
    ) -> Result<()> {
        let project = self.project_row(project_id)?;
        let state = self.state(project_id)?;
        let (photos, scenes): (Vec<(PhotoAsset, Option<u32>)>, Vec<cull_types::Scene>) = {
            let st = state.lock();
            let photos = st
                .assets
                .iter()
                .filter(|a| {
                    let mark = st.marks.get(&a.id).copied().unwrap_or_default();
                    let d = final_decision(st.assessments.get(&a.id).map(|x| x.verdict), &mark);
                    d == FinalDecision::Selected
                        || (request.which == UploadWhich::SelectedAndToReview && d == FinalDecision::ToReview)
                })
                .map(|a| (a.clone(), st.scene_of.get(&a.id).copied()))
                .collect();
            (photos, st.scenes.clone())
        };
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
            let needed: u64 = photos
                .iter()
                .map(|(a, _)| {
                    std::iter::once(&a.primary)
                        .chain(a.companions.iter())
                        .find(|f| f.kind == FileKind::Jpeg)
                        .map(|f| f.size)
                        .unwrap_or(a.primary.size / 3)
                })
                .sum();
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
                    let result = upload_bytes(asset).and_then(|(name, bytes)| {
                        let n = bytes.len() as u64;
                        match target.upload(client, folder, &name, bytes.clone()) {
                            // Folder deleted meanwhile: the gallery root still works.
                            Err(e) if e.code() == "folder_not_found" => target.upload(client, None, &name, bytes),
                            other => other,
                        }
                        .map(|_| n)
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
