//! Integration tests of cull-ml.
//!
//! Tests that need the downloaded models (`node scripts/fetch-models.mjs`)
//! skip with a message when they are absent; tests on real photos also need
//! `samples/faces/` (gitignored, see README) and skip
//! otherwise. Override the folders with `PELLI_CULL_MODELS` and
//! `PELLI_CULL_SAMPLES`.

use cull_ml::{Acceleration, MlError, ModelSet, RuntimeOptions};
use image::{Rgb, RgbImage};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

fn models_dir() -> PathBuf {
    std::env::var_os("PELLI_CULL_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models"))
}

fn samples_dir() -> PathBuf {
    std::env::var_os("PELLI_CULL_SAMPLES")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/faces"))
}

fn models() -> Option<&'static ModelSet> {
    static SET: OnceLock<Option<ModelSet>> = OnceLock::new();
    SET.get_or_init(|| {
        if !models_dir().join("face_detection_yunet_2023mar.onnx").is_file() {
            eprintln!("SKIP: models not downloaded (node scripts/fetch-models.mjs)");
            return None;
        }
        let opts = RuntimeOptions {
            acceleration: Acceleration::CpuOnly,
            intra_threads: 1,
            text_search: true,
            checksum_cache_dir: None,
            accelerator_cache_dir: None,
            memory_bytes: None,
        };
        Some(ModelSet::load(&models_dir(), &opts).expect("models present but failed to load"))
    })
    .as_ref()
}

fn sample(name: &str) -> Option<RgbImage> {
    let p = samples_dir().join(name);
    if !p.is_file() {
        eprintln!("SKIP: sample {name} absent");
        return None;
    }
    Some(image::open(&p).expect("decode sample").to_rgb8())
}

fn cos(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

#[test]
fn missing_models_dir_is_missing_model() {
    let dir = std::env::temp_dir().join(format!("cull-ml-empty-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let r = ModelSet::load(&dir, &RuntimeOptions::default());
    assert!(matches!(r, Err(MlError::MissingModel(ref f)) if f.contains("yunet")), "{:?}", r.err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn corrupt_model_is_checksum_error() {
    let dir = std::env::temp_dir().join(format!("cull-ml-corrupt-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("face_detection_yunet_2023mar.onnx"), b"definitely not onnx").unwrap();
    let r = ModelSet::load(&dir, &RuntimeOptions::default());
    assert!(matches!(r, Err(MlError::Checksum(_))), "{:?}", r.err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_blendshapes_the_eye_aspect_ratio_decides() {
    let src = models_dir();
    let files = ["face_detection_yunet_2023mar.onnx", "face_landmarks_detector_Nx3x256x256.onnx"];
    if !files.iter().all(|f| src.join(f).is_file()) || !src.join("face_blendshapes_v2.onnx").is_file() {
        eprintln!("SKIP: models not downloaded");
        return;
    }
    let Some(img) = sample("portrait_00.jpg").or_else(|| photo("portrait-bokeh-56mm-a.jpg")) else { return };
    let dir = std::env::temp_dir().join(format!("cull-ml-no-blendshapes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for f in files {
        std::fs::copy(src.join(f), dir.join(f)).unwrap();
    }
    let ear = ModelSet::load(&dir, &RuntimeOptions::default()).expect("load without blendshapes");
    assert!(ear.capabilities().eyes);
    assert!(!ear.models_id().contains("blendshapes"), "{}", ear.models_id());
    let face = ear.analyze(&img).unwrap().faces.into_iter().next().expect("a face");
    assert!(face.expression.is_none());
    assert!(face.right_eye_open.is_some() || face.left_eye_open.is_some(), "EAR fallback");
    if let Some(m) = models() {
        assert!(m.models_id().contains("blendshapes-v2"), "{}", m.models_id());
        let full = m.analyze(&img).unwrap().faces.into_iter().next().expect("a face");
        assert!(full.expression.is_some_and(|x| x.eye_blink_right.is_some()));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn faces_only_models_degrade_gracefully() {
    let src = models_dir().join("face_detection_yunet_2023mar.onnx");
    if !src.is_file() {
        eprintln!("SKIP: models not downloaded");
        return;
    }
    let dir = std::env::temp_dir().join(format!("cull-ml-faces-only-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(&src, dir.join("face_detection_yunet_2023mar.onnx")).unwrap();
    let m = ModelSet::load(&dir, &RuntimeOptions::default()).expect("faces-only load");
    let caps = m.capabilities();
    assert!(caps.faces && !caps.eyes && !caps.embeddings && !caps.aesthetic && !caps.text_search);
    assert_eq!(m.models_id(), "yunet-2023mar");
    assert_eq!(m.accelerator(), "CPU");
    let obs = m.analyze(&RgbImage::from_pixel(320, 240, Rgb([90, 120, 200]))).unwrap();
    assert!(obs.embedding.is_none() && obs.aesthetic.is_none());
    assert!(matches!(m.embed_text("hello"), Err(MlError::MissingModel(_))));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn loads_and_reports_capabilities() {
    let Some(m) = models() else { return };
    let caps = m.capabilities();
    assert!(caps.faces);
    assert_eq!(m.accelerator(), "CPU"); // CpuOnly
    assert!(m.models_id().starts_with("yunet-2023mar"), "{}", m.models_id());
    eprintln!("models_id = {} caps = {caps:?}", m.models_id());
}

#[test]
fn degenerate_images_never_fail() {
    let Some(m) = models() else { return };
    let mut noise = RgbImage::new(97, 61);
    let mut s = 12345u32;
    for p in noise.pixels_mut() {
        s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
        *p = Rgb([(s >> 16) as u8, (s >> 8) as u8, s as u8]);
    }
    let images = [
        RgbImage::new(0, 0),
        RgbImage::new(1, 1),
        RgbImage::new(3, 2000),
        RgbImage::new(4000, 2),
        RgbImage::from_pixel(640, 480, Rgb([0, 0, 0])),
        RgbImage::from_pixel(640, 480, Rgb([255, 255, 255])),
        noise,
    ];
    for img in &images {
        let obs = m.analyze(img).unwrap_or_else(|e| panic!("{}x{}: {e}", img.width(), img.height()));
        assert_eq!(obs.models, m.models_id());
        for f in &obs.faces {
            for v in [f.bbox.x, f.bbox.y, f.bbox.w, f.bbox.h, f.confidence, f.area_ratio] {
                assert!((0.0..=1.0).contains(&v));
            }
        }
        if img.width() > 0 && m.capabilities().embeddings {
            let e = obs.embedding.as_ref().expect("embedding");
            assert_eq!(e.len(), 512);
            assert!((norm(e) - 1.0).abs() < 1e-3);
            let a = obs.aesthetic.expect("aesthetic");
            assert!((0.0..12.0).contains(&a), "{a}");
        }
    }
}

#[test]
fn concurrent_calls_match_sequential() {
    let Some(m) = models() else { return };
    let imgs: Vec<RgbImage> = (0..4u8)
        .map(|k| {
            RgbImage::from_fn(300 + 17 * k as u32, 200, |x, y| Rgb([(x as u8).wrapping_mul(k + 1), y as u8, k * 40]))
        })
        .collect();
    let seq: Vec<_> = imgs.iter().map(|i| m.analyze(i).unwrap()).collect();
    for _ in 0..2 {
        std::thread::scope(|s| {
            let handles: Vec<_> = imgs.iter().map(|i| s.spawn(move || m.analyze(i).unwrap())).collect();
            for (h, expected) in handles.into_iter().zip(&seq) {
                let got = h.join().unwrap();
                assert_eq!(got.faces.len(), expected.faces.len());
                if let (Some(a), Some(b)) = (&got.embedding, &expected.embedding) {
                    assert!(cos(a, b) > 0.9999);
                }
            }
        });
    }
}

#[test]
fn text_embeddings() {
    let Some(m) = models() else { return };
    if !m.capabilities().text_search {
        eprintln!("SKIP: no text model");
        return;
    }
    let a = m.embed_text("a bride holding flowers").unwrap();
    let b = m.embed_text("a bride holding flowers").unwrap();
    let c = m.embed_text("a red sports car on a race track").unwrap();
    assert_eq!(a.len(), 512);
    assert!((norm(&a) - 1.0).abs() < 1e-3);
    assert!(cos(&a, &b) > 0.9999);
    assert!(cos(&a, &c) < 0.9);
    for weird in ["", "   ", "<end_of_text>", &"très long ".repeat(500), "😀🎉 mariée"] {
        let e = m.embed_text(weird).unwrap();
        assert!((norm(&e) - 1.0).abs() < 1e-3);
    }
}

// ---------------------------------------------------------------------------
// Real photos (samples/faces, see README for the list and sources)
// ---------------------------------------------------------------------------

#[test]
fn real_portraits_faces_and_open_eyes() {
    let Some(m) = models() else { return };
    for name in ["portraitm_00.jpg", "portraitm_02.jpg", "portrait_07.jpg", "family_00.jpg"] {
        let Some(img) = sample(name) else { return };
        let obs = m.analyze(&img).unwrap();
        assert!(!obs.faces.is_empty(), "{name}: no face");
        let f = &obs.faces[0];
        assert!(f.confidence > 0.8, "{name}");
        if m.capabilities().eyes {
            assert_eq!(f.eyes_closed(0.5), Some(false), "{name}: {f:?}");
        }
    }
}

#[test]
fn real_portraits_mesh_reading_and_detail_image() {
    let Some(m) = models() else { return };
    if !m.capabilities().eyes {
        return;
    }
    let Some(img) = sample("portraitm_00.jpg") else { return };
    let f = m.analyze(&img).unwrap().faces.remove(0);
    assert!(f.mesh_presence.is_some_and(|p| p > 0.9), "{f:?}");
    let pose = f.pose.expect("pose");
    assert!(pose.yaw.abs() < 45.0 && pose.pitch.abs() < 45.0 && pose.mouth_open < 1.0, "{pose:?}");
    // The same portrait in a small preview: the face is small, the engine
    // is asked for a larger decode once, and the mesh reads it there.
    let (w, h) = img.dimensions();
    let small = image::imageops::resize(&img, w / 6, h / 6, image::imageops::FilterType::Triangle);
    let calls = std::cell::Cell::new(0);
    let detail = |long: u32| {
        calls.set(calls.get() + 1);
        assert!(long > w.max(h) / 6, "{long}");
        Some(&img)
    };
    let obs = m.analyze_with_detail(&small, &detail).unwrap();
    let face = &obs.faces[0];
    if face.landmarks[1].x - face.landmarks[0].x < 40.0 / 1440.0 * 6.0 {
        assert_eq!(calls.get(), 1);
        assert!(face.mesh_presence.is_some_and(|p| p > 0.9) && face.right_eye_open.is_some(), "{face:?}");
    }
    // Sessions created in advance serve concurrent calls the same way.
    m.prepare(4);
    let again = m.analyze(&img).unwrap();
    assert_eq!(again.faces.len(), m.analyze(&img).unwrap().faces.len());
}

#[test]
fn real_closed_eyes_are_detected() {
    let Some(m) = models() else { return };
    if !m.capabilities().eyes {
        return;
    }
    for name in ["cx_ce_01.jpg", "cx_ce_05.jpg", "cx_ce_06.jpg", "cx_ce_08.jpg", "cx_ce_12.jpg"] {
        let Some(img) = sample(name) else { return };
        let obs = m.analyze(&img).unwrap();
        assert_eq!(obs.faces.first().and_then(|f| f.eyes_closed(0.5)), Some(true), "{name}: {:?}", obs.faces.first());
    }
}

#[test]
fn no_faces_in_landscapes() {
    let Some(m) = models() else { return };
    for name in ["landscape_00.jpg", "landscape_01.jpg", "beach_01.jpg"] {
        let Some(img) = sample(name) else { return };
        assert!(m.analyze(&img).unwrap().faces.is_empty(), "{name}");
    }
}

#[test]
fn similar_frames_vs_unrelated() {
    let Some(m) = models() else { return };
    if !m.capabilities().embeddings {
        return;
    }
    let (Some(a), Some(other)) = (sample("couple_04.jpg"), sample("landscape_01.jpg")) else { return };
    // Simulated next burst frame: 2 % reframing + brighter exposure.
    let (w, h) = a.dimensions();
    let crop = image::imageops::crop_imm(&a, w / 50, h / 50, w - w / 25, h - h / 25).to_image();
    let mut b = image::imageops::resize(&crop, w, h, image::imageops::FilterType::CatmullRom);
    for p in b.pixels_mut() {
        p.0 = p.0.map(|v| v.saturating_add(12));
    }
    let ea = m.analyze(&a).unwrap().embedding.unwrap();
    let eb = m.analyze(&b).unwrap().embedding.unwrap();
    let eo = m.analyze(&other).unwrap().embedding.unwrap();
    assert!(cos(&ea, &eb) > 0.9, "burst {}", cos(&ea, &eb));
    assert!(cos(&ea, &eo) < 0.6, "unrelated {}", cos(&ea, &eo));
}

#[test]
fn text_search_ranks_matching_photo_first() {
    let Some(m) = models() else { return };
    if !m.capabilities().text_search {
        return;
    }
    let names = ["couple_03.jpg", "landscape_00.jpg", "group_06.jpg", "beach_00.jpg", "portraitm_01.jpg"];
    let mut embs = Vec::new();
    for n in names {
        let Some(img) = sample(n) else { return };
        embs.push(m.analyze(&img).unwrap().embedding.unwrap());
    }
    for (query, expected) in [
        ("a bride holding flowers", "couple_03.jpg"),
        ("a mountain landscape", "landscape_00.jpg"),
        ("a large group photo of many people", "group_06.jpg"),
        ("a sunset over the sea", "beach_00.jpg"),
    ] {
        let q = m.embed_text(query).unwrap();
        let best = embs.iter().zip(names).max_by(|a, b| cos(&q, a.0).total_cmp(&cos(&q, b.0))).unwrap().1;
        assert_eq!(best, expected, "{query}");
    }
}

// ---------------------------------------------------------------------------
// Maximum pack (`models/maximum/`, or `PELLI_CULL_MAXIMUM`): skipped when
// absent. The photos come from `samples/photos` (scripts/fetch-calibration-photos.sh).
// ---------------------------------------------------------------------------

fn maximum() -> Option<&'static cull_ml::MaximumModels> {
    static PACK: OnceLock<Option<cull_ml::MaximumModels>> = OnceLock::new();
    PACK.get_or_init(|| {
        let dir =
            std::env::var_os("PELLI_CULL_MAXIMUM").map(PathBuf::from).unwrap_or_else(|| models_dir().join("maximum"));
        if models().is_none() || !dir.join("dinov2-base-int8.onnx").is_file() {
            eprintln!("SKIP: Maximum pack absent ({})", dir.display());
            return None;
        }
        let opts = RuntimeOptions { acceleration: Acceleration::CpuOnly, ..Default::default() };
        Some(cull_ml::MaximumModels::load(&dir, &opts).expect("pack present but failed to load"))
    })
    .as_ref()
}

fn photo(name: &str) -> Option<RgbImage> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/photos").join(name);
    if !p.is_file() {
        eprintln!("SKIP: sample {name} absent");
        return None;
    }
    Some(image::open(&p).expect("decode sample").to_rgb8())
}

#[test]
fn maximum_pack_checks_and_analyses() {
    let (Some(m), Some(max)) = (models(), maximum()) else { return };
    let steps = std::sync::Mutex::new(Vec::new());
    max.check(&|p| steps.lock().unwrap().push(p)).expect("sanity check");
    assert_eq!(steps.lock().unwrap().last(), Some(&1.0));
    let img = RgbImage::from_fn(640, 427, |x, y| Rgb([(x / 3) as u8, (y / 2) as u8, 90]));
    let obs = m.analyze_maximum(max, &img, &|_| None).unwrap();
    assert!(cull_ml::is_maximum_models_id(&obs.models) && !cull_ml::is_maximum_models_id(&m.models_id()));
    assert!(obs.models.contains("dinov2") && obs.models.contains("laion-v2-l14"), "{}", obs.models);
    let (e, s) = (obs.embedding.unwrap(), obs.search_embedding.unwrap());
    assert_eq!((e.len(), s.len()), (768, 768));
    assert!((norm(&e) - 1.0).abs() < 1e-3 && (norm(&s) - 1.0).abs() < 1e-3);
    assert!(obs.aesthetic.is_some_and(|a| (0.0..12.0).contains(&a)));
    assert!(obs.subject.is_none(), "a gradient has no person");
    let dir = std::env::var_os("PELLI_CULL_MAXIMUM").map(PathBuf::from).unwrap_or_else(|| models_dir().join("maximum"));
    let text = cull_ml::MaximumText::load(&dir, &RuntimeOptions::default()).unwrap();
    assert_eq!(text.embed_text("a dog").unwrap().len(), 768);
}

#[test]
fn maximum_finds_people_and_group_faces() {
    let (Some(m), Some(max)) = (models(), maximum()) else { return };
    // A person seen from the back, no face: the subject region.
    if let Some(img) = photo("wedding-gowns-white.jpg") {
        let obs = m.analyze_maximum(max, &img, &|_| None).unwrap();
        eprintln!(
            "gowns: {} faces, subject {:?}",
            obs.faces.len(),
            obs.subject.as_ref().map(|s| (s.kind, s.area_ratio))
        );
    }
    // A group: RetinaFace adds faces YuNet missed, never fewer.
    if let Some(img) = photo("wedding-party-group.jpg") {
        let std = m.analyze(&img).unwrap();
        let obs = m.analyze_maximum(max, &img, &|_| None).unwrap();
        eprintln!("group: {} faces in Standard, {} in Maximum", std.faces.len(), obs.faces.len());
        assert!(obs.faces.len() >= std.faces.len());
    }
}
