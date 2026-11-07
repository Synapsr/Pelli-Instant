//! Whole-workflow test of the engine on a synthetic shoot, without models:
//! open → analyse → recompute → marks → export (XMP, copy, move, list) →
//! reopen. Runs in CI on every platform.

use cull_engine::cull_types::{CullSettings, Flag, Precision, Sensitivity};
use cull_engine::view::{
    CopySelectedOptions, ExportOptions, MarkPatch, MoveRejectedOptions, PerDecision, XmpExportOptions,
};
use cull_engine::{Engine, EngineConfig, EventSink};
use image::{Rgb, RgbImage};
use parking_lot::Mutex;
use std::path::Path;
use std::sync::Arc;

#[derive(Default)]
struct Recorder(Mutex<Vec<String>>);

impl EventSink for Recorder {
    fn emit(&self, event: &str, _payload: serde_json::Value) {
        self.0.lock().push(event.to_string());
    }
}

/// Textured, sharp scene (seeded so frames differ).
fn scene(seed: u32, w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let v = ((x / 7 + y / 5 + seed) % 2) * 160 + ((x * 31 + y * 17 + seed * 13) % 60);
        Rgb([v as u8, (v / 2 + 40) as u8, (255 - v) as u8])
    })
}

/// Sharp concentric rings: unlike `scene`, never similar to the others.
fn rings(w: u32, h: u32) -> RgbImage {
    RgbImage::from_fn(w, h, |x, y| {
        let (dx, dy) = (x as i64 - w as i64 / 3, y as i64 - h as i64 / 2);
        let r = ((dx * dx + dy * dy) as f64).sqrt() as u32;
        let v = ((r / 9) % 2) * 170 + (y % 40);
        Rgb([(255 - v) as u8, v as u8, (v / 3 + 60) as u8])
    })
}

fn save(img: &RgbImage, path: &Path) {
    std::fs::write(path, cull_engine::cull_decode::encode_jpeg(img, 90)).unwrap();
}

#[test]
fn full_workflow_without_models() {
    let tmp = tempfile::tempdir().unwrap();
    let shoot = tmp.path().join("Mariage");
    std::fs::create_dir_all(&shoot).unwrap();
    for i in 0..4 {
        save(&scene(i * 7, 1200, 800), &shoot.join(format!("IMG_{i:04}.jpg")));
    }
    // An exact copy, a blurred frame and a black frame.
    std::fs::copy(shoot.join("IMG_0000.jpg"), shoot.join("IMG_0000-copy.jpg")).unwrap();
    let blurred = image::imageops::blur(&scene(99, 1200, 800), 3.0);
    save(&blurred, &shoot.join("IMG_0100.jpg"));
    save(&RgbImage::from_pixel(1200, 800, Rgb([3, 3, 3])), &shoot.join("IMG_0101.jpg"));
    // RAW + JPEG pair: the RAW is unreadable here, the analysis falls back
    // to its JPEG twin; the XMP goes next to the RAW.
    std::fs::write(shoot.join("IMG_0200.dng"), b"II*\0not a real raw").unwrap();
    save(&rings(1200, 800), &shoot.join("IMG_0200.jpg"));
    std::fs::write(shoot.join("notes.txt"), b"not a photo").unwrap();

    let events = Arc::new(Recorder::default());
    let engine = Engine::open(EngineConfig {
        data_dir: tmp.path().join("data"),
        cache_dir: tmp.path().join("data/cache"),
        models_dir: tmp.path().join("no-models"),
        pack_dir: None,
        url_mapper: Arc::new(|p: &Path| format!("file://{}", p.display())),
        events: events.clone(),
        secrets: None,
    })
    .unwrap();
    assert!(engine.models().is_none(), "no models: measures only");

    let project = engine.open_folder(&shoot, true).unwrap();
    assert_eq!(project.photo_count, 8);
    assert_eq!(project.raw_count, 1);

    let summary = engine.analyze_blocking(&project.id, CullSettings::default()).unwrap();
    assert_eq!((summary.analyzed, summary.failed), (8, 0));
    let names: Vec<String> = events.0.lock().clone();
    assert!(names.iter().any(|n| n == "analysis-photo"));
    assert_eq!(names.last().map(String::as_str), Some("analysis-done"));

    let data = engine.get_project(&project.id).unwrap();
    let by_name = |name: &str| data.photos.iter().find(|p| p.file_name == name).unwrap().clone();
    for p in &data.photos {
        assert!(p.thumb_url.is_some(), "thumbnail for {}", p.file_name);
        assert!(p.assessment.is_some());
    }
    let reasons = |name: &str| by_name(name).assessment.unwrap().reasons;
    use cull_engine::cull_types::{Reason, Verdict};
    let blur = by_name("IMG_0100.jpg").assessment.unwrap();
    assert!(
        blur.verdict != Verdict::Keep && blur.reasons.iter().any(|r| matches!(r, Reason::Blurry | Reason::MotionBlur)),
        "{blur:?}"
    );
    assert!(reasons("IMG_0101.jpg").contains(&Reason::Underexposed), "{:?}", reasons("IMG_0101.jpg"));
    // The exact copy and its original form one group: only one is kept.
    let copy = by_name("IMG_0000-copy.jpg").assessment.unwrap();
    let orig = by_name("IMG_0000.jpg").assessment.unwrap();
    assert!(copy.group_id.is_some() && copy.group_id == orig.group_id);
    assert_ne!(copy.verdict == Verdict::Keep, orig.verdict == Verdict::Keep);

    // Re-running is incremental: nothing left to analyse.
    let again = engine.analyze_blocking(&project.id, CullSettings::default()).unwrap();
    assert_eq!(again.analyzed, 8);
    assert_eq!(engine.get_project(&project.id).unwrap().project.precision, Some(Precision::Standard));

    // Maximum precision without the pack: the analysis runs in Standard,
    // the setting stays, the pack is absent and resumable from nothing.
    let patch = cull_engine::settings::SettingsPatch { precision: Some(Precision::Maximum), ..Default::default() };
    assert_eq!(engine.set_settings(patch).unwrap().precision, Precision::Maximum);
    let pack = engine.model_pack();
    assert_eq!(pack.state, cull_engine::modelpack::ModelPackState::Absent);
    assert!(pack.size_bytes > 700_000_000 && pack.disk_bytes == pack.size_bytes);
    let fallback = engine.analyze_blocking(&project.id, CullSettings::default()).unwrap();
    assert_eq!(fallback.precision, Precision::Standard);
    assert!(!engine.maximum_ready());
    assert_eq!(engine.remove_model_pack().unwrap().state, cull_engine::modelpack::ModelPackState::Absent);

    // Sensitivity changes are instant and persisted.
    let strict = CullSettings { sensitivity: Sensitivity::Strict, ..CullSettings::default() };
    let rec = engine.recompute(&project.id, strict.clone()).unwrap();
    assert_eq!(rec.project.settings.sensitivity, Sensitivity::Strict);

    // The photographer's flag wins over the AI.
    let blurred_id = by_name("IMG_0100.jpg").id;
    let views = engine
        .set_marks(
            &project.id,
            std::slice::from_ref(&blurred_id),
            MarkPatch { flag: Some(Flag::Pick), rating: Some(4), label: None },
        )
        .unwrap();
    assert_eq!(views[0].decision, cull_engine::cull_types::FinalDecision::Selected);
    let pair_id = by_name("IMG_0200.dng").id;
    engine
        .set_marks(
            &project.id,
            std::slice::from_ref(&pair_id),
            MarkPatch { flag: Some(Flag::Pick), rating: Some(5), label: None },
        )
        .unwrap();

    // Export everything, the selection into the shoot folder itself.
    let dest = shoot.join("Selection");
    let report = engine
        .export_project(
            &project.id,
            &ExportOptions {
                xmp: XmpExportOptions {
                    enabled: true,
                    rating: PerDecision { selected: 3, to_review: 2, rejected: -1 },
                    label: PerDecision {
                        selected: cull_engine::cull_types::ColorLabel::Green,
                        to_review: cull_engine::cull_types::ColorLabel::Yellow,
                        rejected: cull_engine::cull_types::ColorLabel::Red,
                    },
                    keep_user_marks: true,
                },
                copy_selected: CopySelectedOptions {
                    enabled: true,
                    destination: Some(dest.to_string_lossy().into_owned()),
                    include_raw: true,
                    include_jpeg: true,
                    keep_structure: false,
                },
                move_rejected: MoveRejectedOptions { enabled: true, folder_name: "_Rejets Pelli".into() },
                file_list: true,
            },
        )
        .unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    // One sidecar: next to the RAW only, never next to the JPEGs.
    assert_eq!(report.xmp_written, 1);
    assert!(!shoot.join("IMG_0100.xmp").exists() && !dest.join("IMG_0100.xmp").exists());
    assert!(report.copied >= 2 && report.moved >= 1);

    // The picked pair carries the user's 5 stars, copied with both files.
    let xmp = std::fs::read_to_string(dest.join("IMG_0200.xmp")).unwrap();
    assert!(xmp.contains("Rating") && xmp.contains('5'), "{xmp}");
    assert!(dest.join("IMG_0200.dng").exists() && dest.join("IMG_0200.jpg").exists());
    assert!(dest.join("IMG_0100.jpg").exists());
    // Rejects moved next to their folder, originals never deleted.
    let rejects = shoot.join("_Rejets Pelli");
    assert!(rejects.join("IMG_0101.jpg").exists());
    let list = std::fs::read_to_string(report.file_list_path.unwrap()).unwrap();
    assert!(list.contains("IMG_0100"));

    // Reopening the folder keeps marks and imports neither the rejects nor
    // the copy of the selection.
    let reopened = engine.open_folder(&shoot, true).unwrap();
    assert_eq!(reopened.id, project.id);
    let data = engine.get_project(&project.id).unwrap();
    assert!(data.photos.iter().all(|p| !p.rel_path.starts_with("_Rejets") && !p.rel_path.starts_with("Selection")));
    assert!(dest.join(cull_engine::cull_decode::EXPORT_MARKER).exists());
    let picked = data.photos.iter().find(|p| p.id == blurred_id).unwrap();
    assert_eq!(picked.mark.rating, 4);
}

#[test]
fn new_projects_start_from_the_default_criteria() {
    use cull_engine::cull_types::Reason;
    use cull_engine::settings::SettingsPatch;
    let tmp = tempfile::tempdir().unwrap();
    let shoot = tmp.path().join("Portraits");
    std::fs::create_dir_all(&shoot).unwrap();
    save(&scene(3, 1200, 800), &shoot.join("IMG_0001.jpg"));
    save(&rings(1200, 800), &shoot.join("IMG_0002.jpg"));
    std::fs::copy(shoot.join("IMG_0001.jpg"), shoot.join("IMG_0001-copy.jpg")).unwrap();
    let config = || EngineConfig {
        data_dir: tmp.path().join("data"),
        cache_dir: tmp.path().join("data/cache"),
        models_dir: tmp.path().join("no-models"),
        pack_dir: None,
        url_mapper: Arc::new(|p: &Path| format!("file://{}", p.display())),
        events: Arc::new(Recorder::default()),
        secrets: None,
    };
    let engine = Engine::open(config()).unwrap();
    assert_eq!(engine.get_settings().cull, CullSettings::default());

    let artistic = CullSettings { artistic_blur: true, detect_motion_blur: false, ..CullSettings::default() };
    let saved = engine.set_settings(SettingsPatch { cull: Some(artistic.clone()), ..Default::default() }).unwrap();
    assert_eq!(saved.cull, artistic);
    let project = engine.open_folder(&shoot, true).unwrap();
    assert_eq!(project.settings, artistic);

    // Not analysed yet: the project follows the defaults when they change.
    let strict = CullSettings { sensitivity: Sensitivity::Strict, ..artistic.clone() };
    engine.set_settings(SettingsPatch { cull: Some(strict.clone()), ..Default::default() }).unwrap();
    assert_eq!(engine.open_folder(&shoot, true).unwrap().settings, strict);

    // Once analysed, its own criteria stay.
    let own = CullSettings { detect_exact_duplicates: false, group_similar: false, ..CullSettings::default() };
    engine.analyze_blocking(&project.id, own.clone()).unwrap();
    engine.set_settings(SettingsPatch { cull: Some(CullSettings::default()), ..Default::default() }).unwrap();
    let reopened = engine.open_folder(&shoot, true).unwrap();
    assert_eq!(reopened.settings, own);
    let data = engine.get_project(&project.id).unwrap();
    assert!(data.photos.iter().all(|p| !p.assessment.as_ref().unwrap().reasons.contains(&Reason::ExactDuplicate)));
    // Exact copies detected again after a recompute.
    let data = engine.recompute(&project.id, CullSettings::default()).unwrap();
    let copies =
        data.photos.iter().filter(|p| p.assessment.as_ref().unwrap().reasons.contains(&Reason::ExactDuplicate));
    assert_eq!(copies.count(), 1);

    // The defaults survive a restart.
    engine.set_settings(SettingsPatch { cull: Some(strict.clone()), ..Default::default() }).unwrap();
    drop(engine);
    assert_eq!(Engine::open(config()).unwrap().get_settings().cull, strict);
}

fn engine_in(dir: &Path) -> Arc<Engine> {
    Engine::open(EngineConfig {
        data_dir: dir.join("data"),
        cache_dir: dir.join("data/cache"),
        models_dir: dir.join("no-models"),
        pack_dir: None,
        url_mapper: Arc::new(|p: &Path| format!("file://{}", p.display())),
        events: Arc::new(Recorder::default()),
        secrets: None,
    })
    .unwrap()
}

#[test]
fn an_empty_reopening_keeps_the_project() {
    let tmp = tempfile::tempdir().unwrap();
    let shoot = tmp.path().join("Shoot");
    std::fs::create_dir_all(shoot.join("Card 1")).unwrap();
    save(&scene(1, 600, 400), &shoot.join("Card 1/IMG_0001.jpg"));
    let engine = engine_in(tmp.path());
    let project = engine.open_folder(&shoot, true).unwrap();
    let id = engine.get_project(&project.id).unwrap().photos[0].id.clone();
    let pick = MarkPatch { flag: Some(Flag::Pick), rating: Some(5), label: None };
    engine.set_marks(&project.id, std::slice::from_ref(&id), pick).unwrap();

    // Without its sub-folders, the folder has no photo: nothing is removed.
    let err = engine.open_folder(&shoot, false).unwrap_err();
    assert_eq!(err.code(), "noPhotos");
    assert_eq!(engine.list_projects().unwrap().len(), 1);
    let reopened = engine.open_folder(&shoot, true).unwrap();
    assert_eq!(reopened.id, project.id);
    let data = engine.get_project(&project.id).unwrap();
    assert_eq!(data.photos[0].mark.rating, 5);

    // A folder never opened and empty creates nothing.
    std::fs::create_dir_all(tmp.path().join("Empty")).unwrap();
    assert_eq!(engine.open_folder(&tmp.path().join("Empty"), true).unwrap_err().code(), "noPhotos");
    assert_eq!(engine.list_projects().unwrap().len(), 1);
}

#[test]
fn an_unreadable_file_keeps_its_previous_analysis() {
    let tmp = tempfile::tempdir().unwrap();
    let shoot = tmp.path().join("Shoot");
    std::fs::create_dir_all(&shoot).unwrap();
    save(&scene(2, 600, 400), &shoot.join("IMG_0001.jpg"));
    save(&rings(600, 400), &shoot.join("IMG_0002.jpg"));
    let engine = engine_in(tmp.path());
    let project = engine.open_folder(&shoot, true).unwrap();
    assert_eq!(engine.analyze_blocking(&project.id, CullSettings::default()).unwrap().analyzed, 2);

    // The file can no longer be read (same size and date: nothing to
    // re-analyse) and its thumbnail is gone.
    let path = shoot.join("IMG_0001.jpg");
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let len = std::fs::metadata(&path).unwrap().len() as usize;
    std::fs::write(&path, vec![0u8; len]).unwrap();
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(modified).unwrap();
    engine.clear_cache().unwrap();

    let summary = engine.analyze_blocking(&project.id, CullSettings::default()).unwrap();
    assert_eq!((summary.analyzed, summary.failed), (2, 0));
    let data = engine.get_project(&project.id).unwrap();
    let photo = data.photos.iter().find(|p| p.file_name == "IMG_0001.jpg").unwrap();
    assert!(photo.assessment.is_some() && photo.error.is_none(), "{photo:?}");
}
