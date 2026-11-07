//! The optional **Maximum** pack: bigger models, slower, finer results.
//!
//! Five files downloaded by the app into its data folder (`bundle:
//! "maximum"` in `models/manifest.json`, ~777 MB), all run on the CPU:
//!
//! * **DINOv2 ViT-B/14** int8 (Meta, Apache-2.0): CLS token of the whole
//!   photo squashed to 224x224, ImageNet normalisation, L2-normalised. The
//!   grouping embedding (bursts, near-duplicates): on the calibration set it
//!   separates duplicates from different scenes by +0.25 of cosine, CLIP
//!   B/32 by +0.11, CLIP L/14 by +0.05. Fixed input shape (session dimension
//!   overrides).
//! * **CLIP ViT-L/14** int8 image tower (OpenAI, MIT), same letterboxed
//!   preprocessing as ViT-B/32: text search and the **LAION V2** aesthetic
//!   head (compiled in). Its fp16 text tower ([`MaximumText`]) is loaded
//!   only when a project analysed in Maximum is searched.
//! * **RF-DETR-Seg Small** (Roboflow, Apache-2.0, 384x384): instance masks of
//!   people (COCO 1) and animals (COCO 16-25). Run on photos without a
//!   usable face: the largest instance is the subject region whose focus and
//!   exposure judge the photo (a back-lit figure seen from behind).
//! * **RetinaFace R50** (MIT): a second face detector, run at 1280 px on
//!   group photos only (YuNet found 3+ faces, or a small one): it finds the
//!   faces YuNet misses in crowds and profiles (26 against 18 on a group).
//!   Its new faces go through the same mesh and blendshapes.
//!
//! Never the Neural Engine: in fp16 it silently returns wrong values for
//! DINOv2, RF-DETR and CLIP L/14. The pack is CPU-only (int8 and fp16-stored
//! weights computed in fp32), which on Apple Silicon is also the fastest
//! without minutes of CoreML compilation.
//!
//! Memory: CLIP L/14 keeps 2 sessions (3 with ≥ 8 cores and ≥ 12 GB) of a
//! few intra-op threads each, RF-DETR and RetinaFace 2, DINOv2 up to 4,
//! whatever the number of analysis workers, and the engine runs at most
//! [`MaximumModels::max_workers`] workers with the pack (more would only
//! wait, holding decoded photos). [`MaximumModels`] is loaded when an
//! analysis needs it and dropped after.

use crate::clip::{self, ClipText, ClipVision};
use crate::faces::{nms, Face};
use crate::imgproc::{resize, RgbView};
use crate::manifest;
use crate::runtime::{
    build_first, compiled_accelerators, default_pool_size, output_f32, CoreMlUnits, ModelOpts, ModelSource, Provider,
    SessionPool,
};
use crate::{guard, Acceleration, MlError, Result, RuntimeOptions};
use cull_types::{NormRect, SubjectKind, SubjectMask, SubjectObservation};
use fast_image_resize::{FilterType, ResizeAlg};
use ort::session::Session;
use ort::value::TensorRef;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Model ids of the pack in `models/manifest.json`.
pub(crate) const DINO: &str = "dinov2-b14-int8";
pub(crate) const CLIP_VISION: &str = "clip-l14-vision-int8";
pub(crate) const CLIP_TEXT: &str = "clip-l14-text";
pub(crate) const RFDETR: &str = "rfdetr-seg-s";
pub(crate) const RETINA: &str = "retinaface-r50";

const DINO_SIZE: usize = 224;
const DINO_DIM: usize = 768;
const DINO_DIMS: &[(&str, i64)] = &[("batch_size", 1), ("num_channels", 3), ("height", 224), ("width", 224)];
const CLIP_DIMS: &[(&str, i64)] = &[("batch_size", 1), ("num_channels", 3), ("height", 224), ("width", 224)];
const TEXT_DIMS: &[(&str, i64)] = &[("batch_size", 1), ("sequence_length", 77)];
const IMAGENET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const IMAGENET_STD: [f32; 3] = [0.229, 0.224, 0.225];

const RFDETR_SIZE: usize = 384;
const RFDETR_QUERIES: usize = 100;
const RFDETR_CLASSES: usize = 91;
const RFDETR_MASK: usize = 96;
/// Detections kept from this score (sigmoid of the class logit).
pub(crate) const RFDETR_SCORE: f32 = 0.5;
/// COCO category ids: person, then bird … giraffe.
const COCO_PERSON: usize = 1;
const COCO_ANIMALS: std::ops::RangeInclusive<usize> = 16..=25;
/// Smallest subject region (share of the frame) worth judging.
pub(crate) const SUBJECT_MIN_AREA: f32 = 0.01;

/// Long side of the RetinaFace pass.
const RETINA_LONG: u32 = 1280;
const RETINA_STEPS: [usize; 3] = [8, 16, 32];
const RETINA_SIZES: [[f32; 2]; 3] = [[16.0, 32.0], [64.0, 128.0], [256.0, 512.0]];
const RETINA_MEAN_BGR: [f32; 3] = [104.0, 117.0, 123.0];
/// Faces kept from this score; overlapping detections suppressed above this
/// IoU (NMS 0.4 as in the repository). At 0.8, 8 of the 13 faces it added
/// on the calibration shoots were not faces for the mesh (watermark
/// letters, hands).
pub(crate) const RETINA_SCORE: f32 = 0.9;
const RETINA_NMS: f32 = 0.4;
/// A RetinaFace face overlapping a YuNet face by more than this is the same.
const SAME_FACE_IOU: f32 = 0.3;
/// RetinaFace runs when YuNet found this many faces (a group: on the
/// calibration shoots it added no face to photos of one or two people), or a
/// face smaller than `RETINA_SMALL_FACE` of the photo's long side.
const RETINA_GROUP_FACES: usize = 3;
const RETINA_SMALL_FACE: f32 = 40.0 / 640.0;

/// Sessions of the models that run on part of the photos only (RF-DETR,
/// RetinaFace).
const PARTIAL_SESSIONS: usize = 2;

/// Revisions of the readings, part of `models_id`.
const DINO_TAG: &str = "sq";

/// The CLIP ViT-L/14 text tower of the pack, for searching projects
/// analysed in Maximum precision (loaded separately: ~250 MB, only when such
/// a project is searched).
pub struct MaximumText {
    model: ClipText,
}

impl MaximumText {
    /// Load the text tower from the pack folder `dir` (verified).
    pub fn load(dir: &Path, opts: &RuntimeOptions) -> Result<MaximumText> {
        guard("MaximumText::load", || {
            manifest::set_cache_dir(opts.checksum_cache_dir.clone());
            let (_, path) = verified(dir, CLIP_TEXT)?;
            Self::from_file(path, heavy_pool(opts.memory_bytes).1.max(2))
        })
    }

    fn from_file(path: PathBuf, threads: usize) -> Result<MaximumText> {
        let o = ModelOpts { dims: TEXT_DIMS, ..ModelOpts::cpu(threads) };
        let (session, _) = build_first("clip-l14-text", ModelSource::File(path), None, &o)?;
        Ok(MaximumText {
            model: ClipText {
                session: Mutex::new(session),
                tokenizer: instant_clip_tokenizer::Tokenizer::new(),
                dim: clip::L14_DIM,
            },
        })
    }

    /// L2-normalised CLIP ViT-L/14 text embedding (compare with the
    /// `search_embedding` of analyses made in Maximum precision).
    pub fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        guard("embed_text", || self.model.embed(text))
    }
}

/// The loaded Maximum pack (see the module docs). `Send + Sync`.
pub struct MaximumModels {
    dino: SessionPool,
    clip: ClipVision,
    rfdetr: SessionPool,
    retina: SessionPool,
    /// The text tower's file, loaded only by [`MaximumModels::check`].
    text_path: PathBuf,
    text_threads: usize,
    /// Sessions of CLIP L/14.
    heavy_sessions: usize,
    tags: Vec<String>,
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MaximumModels>();
};

/// One file of the Maximum pack, for the downloader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFile {
    pub id: String,
    pub file: String,
    pub url: String,
    pub mirrors: Vec<String>,
    pub sha256: String,
    pub size: u64,
}

/// The files of the Maximum pack (from the compiled-in manifest).
pub fn maximum_pack_files() -> Vec<PackFile> {
    manifest::bundle("maximum")
        .map(|v| {
            v.into_iter()
                .map(|e| PackFile {
                    id: e.id.clone(),
                    file: e.file.clone(),
                    url: e.url.clone(),
                    mirrors: e.mirrors.clone(),
                    sha256: e.sha256.clone(),
                    size: e.size,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Sessions of the heavy models: few, with a few threads each (memory is
/// per session, not per thread): 2 below 12 GB of memory or 8 cores, 3
/// above.
fn heavy_pool(memory_bytes: Option<u64>) -> (usize, usize) {
    let cores = default_pool_size();
    let big = cores >= 8 && memory_bytes.is_none_or(|m| m >= 12 << 30);
    let sessions = if big { 3 } else { 2 };
    (sessions, (cores / sessions).clamp(1, 4))
}

fn verified(dir: &Path, id: &str) -> Result<(&'static str, PathBuf)> {
    match manifest::verify_file(dir, id)? {
        Some((e, path)) => Ok((e.tag.as_str(), path)),
        None => Err(MlError::MissingModel(manifest::entry(id).map(|e| e.file.clone()).unwrap_or_else(|_| id.into()))),
    }
}

impl MaximumModels {
    /// Load the pack from `dir` (file names and SHA-256 from the manifest,
    /// verified in a streaming pass and cached like the other models). Every
    /// file is needed: `MissingModel` / `Checksum` otherwise. Creates one
    /// session per model; the text tower is only checked here (searches load
    /// it through [`MaximumText`]).
    pub fn load(dir: &Path, opts: &RuntimeOptions) -> Result<MaximumModels> {
        guard("MaximumModels::load", || Self::load_inner(dir, opts))
    }

    fn load_inner(dir: &Path, opts: &RuntimeOptions) -> Result<MaximumModels> {
        manifest::set_cache_dir(opts.checksum_cache_dir.clone());
        let mut tags = Vec::new();
        let (heavy, heavy_threads) = heavy_pool(opts.memory_bytes);
        let intra = opts.intra_threads.clamp(1, 64);

        let (tag, path) = verified(dir, DINO)?;
        let o = ModelOpts { dims: DINO_DIMS, ..ModelOpts::cpu(intra) };
        let (first, spec) = build_first("dinov2", ModelSource::File(path), None, &o)?;
        let dino = SessionPool::new(first, spec, None, default_pool_size().min(4));
        tags.push(format!("{tag}-{DINO_TAG}"));

        let (tag, path) = verified(dir, CLIP_VISION)?;
        let prepacked = ort::session::builder::PrepackedWeights::new();
        let o = ModelOpts { dims: CLIP_DIMS, prepacked: Some(&prepacked), ..ModelOpts::cpu(heavy_threads) };
        let (first, spec) = build_first("clip-l14", ModelSource::File(path), None, &o)?;
        let clip = ClipVision { pool: SessionPool::new(first, spec, Some(prepacked), heavy), dim: clip::L14_DIM };
        tags.push(format!("{tag}-{}", clip::PREPROCESS_TAG));
        tags.push("laion-v2-l14".to_string());

        let (_, text_path) = verified(dir, CLIP_TEXT)?;

        // RF-DETR on the GPU only when the accelerator is asked for
        // explicitly (benchmarks): CoreML takes 1 014 of its 1 020 nodes but
        // compiles for 25-45 s, and its fp16 path is wrong on the Neural
        // Engine: GPU and CPU only.
        let (tag, path) = verified(dir, RFDETR)?;
        let accels: Vec<Provider> = match opts.acceleration {
            Acceleration::Accelerator => compiled_accelerators(),
            _ => Vec::new(),
        };
        let o = ModelOpts {
            candidates: &accels,
            cache_dir: opts.accelerator_cache_dir.as_deref(),
            coreml_units: CoreMlUnits::CpuAndGpu,
            ..ModelOpts::cpu(heavy_threads.min(2))
        };
        let (first, spec) = build_first("rfdetr", ModelSource::File(path), None, &o)?;
        // RF-DETR and RetinaFace only run on part of the photos: 2 sessions.
        let rfdetr = SessionPool::new(first, spec, None, PARTIAL_SESSIONS);
        tags.push(tag.to_string());

        let (tag, path) = verified(dir, RETINA)?;
        let (first, spec) = build_first("retinaface", ModelSource::File(path), None, &ModelOpts::cpu(heavy_threads))?;
        let retina = SessionPool::new(first, spec, None, PARTIAL_SESSIONS);
        tags.push(tag.to_string());

        Ok(MaximumModels {
            dino,
            clip,
            rfdetr,
            retina,
            text_path,
            text_threads: heavy_threads.max(2),
            heavy_sessions: heavy,
            tags,
        })
    }

    /// Identifiers of the pack's models and readings, part of `models_id`.
    pub(crate) fn tags(&self) -> &[String] {
        &self.tags
    }

    /// Analysis workers worth running with the pack: beyond about two per
    /// CLIP L/14 session they only wait for it, each holding a decoded
    /// photo (and sometimes its full-size version) in memory. 8 on a big
    /// machine, 4 below 12 GB or 8 cores (measured peak with 4 workers:
    /// 3.7 GB on a 211-photo shoot, against 4.4 GB with 8).
    pub fn max_workers(&self) -> usize {
        if self.heavy_sessions >= 3 {
            8
        } else {
            4
        }
    }

    /// Create the sessions `workers` analysis threads need, in parallel.
    pub fn prepare(&self, workers: usize) {
        let workers = workers.max(1);
        std::thread::scope(|s| {
            s.spawn(|| self.dino.warm(workers));
            s.spawn(|| self.clip.pool.warm(workers));
            s.spawn(|| self.rfdetr.warm(2));
            s.spawn(|| self.retina.warm(2));
        });
    }

    /// Run every model once on synthetic inputs and compare their outputs
    /// with reference values computed when the pack was built (ONNX Runtime
    /// 1.31, CPU): a session that loads but computes wrong (a broken
    /// runtime, an accelerator returning garbage) fails here. `progress`
    /// receives 0..1 as the models pass. Loads the text tower too, then
    /// releases it.
    pub fn check(&self, progress: &dyn Fn(f32)) -> Result<()> {
        guard("MaximumModels::check", || {
            let refs: serde_json::Value =
                serde_json::from_str(CHECK_JSON).map_err(|e| MlError::Runtime(format!("maximum-check.json: {e}")))?;
            let reference = |id: &str| -> Result<Vec<f32>> {
                serde_json::from_value(refs[id].clone())
                    .map_err(|e| MlError::Runtime(format!("maximum-check.json {id}: {e}")))
            };
            let steps = 5.0;
            let image = pattern(3, DINO_SIZE, DINO_SIZE, 1.5);
            let cls = self.dino.with(|s| dino_raw(s, &image))?;
            expect_close(DINO, &cls.iter().step_by(12).copied().collect::<Vec<_>>(), &reference(DINO)?)?;
            progress(1.0 / steps);
            let e = self.clip.pool.with(|s| {
                let t = TensorRef::from_array_view(([1usize, 3, 224, 224], &image[..]))
                    .map_err(|e| MlError::Runtime(format!("clip-l14 input: {e}")))?;
                let out = s.run(ort::inputs![t]).map_err(|e| MlError::Runtime(format!("clip-l14: {e}")))?;
                Ok(output_f32(&out, "image_embeds", clip::L14_DIM)?[..clip::L14_DIM].to_vec())
            })?;
            expect_close(CLIP_VISION, &e.iter().step_by(12).copied().collect::<Vec<_>>(), &reference(CLIP_VISION)?)?;
            progress(2.0 / steps);
            let input = pattern(3, RFDETR_SIZE, RFDETR_SIZE, 1.5);
            let logits = self.rfdetr.with(|s| {
                let t = TensorRef::from_array_view(([1usize, 3, RFDETR_SIZE, RFDETR_SIZE], &input[..]))
                    .map_err(|e| MlError::Runtime(format!("rfdetr input: {e}")))?;
                let out = s.run(ort::inputs![t]).map_err(|e| MlError::Runtime(format!("rfdetr: {e}")))?;
                let l = output_f32(&out, "labels", RFDETR_QUERIES * RFDETR_CLASSES)?;
                Ok((0..64).map(|q| l[q * RFDETR_CLASSES + COCO_PERSON]).collect::<Vec<_>>())
            })?;
            expect_close(RFDETR, &logits, &reference(RFDETR)?)?;
            progress(3.0 / steps);
            let input = pattern(3, 64, 96, 50.0);
            let loc = self.retina.with(|s| {
                let t = TensorRef::from_array_view(([1usize, 3, 64, 96], &input[..]))
                    .map_err(|e| MlError::Runtime(format!("retinaface input: {e}")))?;
                let out = s.run(ort::inputs![t]).map_err(|e| MlError::Runtime(format!("retinaface: {e}")))?;
                Ok(output_f32(&out, "loc", 64)?[..64].to_vec())
            })?;
            expect_close(RETINA, &loc, &reference(RETINA)?)?;
            progress(4.0 / steps);
            let text = MaximumText::from_file(self.text_path.clone(), self.text_threads)?;
            let mut ids = vec![0i64; 77];
            ids[..7].copy_from_slice(&[49406, 320, 1125, 539, 320, 1929, 49407]);
            // `embed_ids` normalises: compare directions only.
            let t = text.model.embed_ids(&ids)?;
            expect_close(CLIP_TEXT, &t.iter().step_by(12).copied().collect::<Vec<_>>(), &reference(CLIP_TEXT)?)?;
            progress(1.0);
            Ok(())
        })
    }

    /// L2-normalised DINOv2 CLS embedding of the whole image (squashed).
    pub(crate) fn dino_embed(&self, img: RgbView<'_>) -> Result<Vec<f32>> {
        let small = resize(img, DINO_SIZE as u32, DINO_SIZE as u32, ResizeAlg::Convolution(FilterType::CatmullRom))?;
        let input = normalized_chw(small.view(), IMAGENET_MEAN, IMAGENET_STD);
        let mut cls = self.dino.with(|s| dino_raw(s, &input))?;
        if !clip::l2_normalize(&mut cls) {
            return Err(MlError::Runtime("dinov2: degenerate embedding".into()));
        }
        Ok(cls)
    }

    /// L2-normalised CLIP ViT-L/14 embedding and LAION V2 aesthetic score.
    pub(crate) fn clip_embed(&self, img: RgbView<'_>) -> Result<(Vec<f32>, Option<f32>)> {
        let e = self.clip.embed(img)?;
        let a = clip::aesthetic_l14(&e);
        Ok((e, a))
    }

    /// The subject region: the largest person or animal instance covering at
    /// least `SUBJECT_MIN_AREA` of the frame.
    pub(crate) fn subject(&self, img: RgbView<'_>) -> Result<Option<SubjectObservation>> {
        let small = resize(img, RFDETR_SIZE as u32, RFDETR_SIZE as u32, ResizeAlg::Convolution(FilterType::Bilinear))?;
        let input = normalized_chw(small.view(), IMAGENET_MEAN, IMAGENET_STD);
        self.rfdetr.with(|s| {
            let t = TensorRef::from_array_view(([1usize, 3, RFDETR_SIZE, RFDETR_SIZE], &input[..]))
                .map_err(|e| MlError::Runtime(format!("rfdetr input: {e}")))?;
            let out = s.run(ort::inputs![t]).map_err(|e| MlError::Runtime(format!("rfdetr: {e}")))?;
            let labels = output_f32(&out, "labels", RFDETR_QUERIES * RFDETR_CLASSES)?;
            let masks = output_f32(&out, "masks", RFDETR_QUERIES * RFDETR_MASK * RFDETR_MASK)?;
            Ok(largest_instance(labels, masks))
        })
    }

    /// Faces RetinaFace finds on `work` (≤ 1280 px) that YuNet did not,
    /// in `full` pixel coordinates, when the photo is a group (see the
    /// module docs). `None` when it did not run.
    pub(crate) fn more_faces(&self, full: RgbView<'_>, work: RgbView<'_>, yunet: &[Face]) -> Result<Option<Vec<Face>>> {
        let long = full.w.max(full.h) as f32;
        let wanted = yunet.len() >= RETINA_GROUP_FACES || yunet.iter().any(|f| f.w.max(f.h) < RETINA_SMALL_FACE * long);
        if !wanted {
            return Ok(None);
        }
        let (sx, sy) = (full.w as f32 / work.w as f32, full.h as f32 / work.h as f32);
        let found = self.retina_detect(work)?;
        let new = found
            .into_iter()
            .map(|f| scale_face(f, sx, sy))
            .filter(|f| yunet.iter().all(|y| iou(y, f) <= SAME_FACE_IOU && !centre_inside(y, f)))
            .collect();
        Ok(Some(new))
    }

    /// RetinaFace on `img` (its long side ≤ `RETINA_LONG`), in `img` pixels.
    fn retina_detect(&self, img: RgbView<'_>) -> Result<Vec<Face>> {
        let (w, h) = (img.w as usize, img.h as usize);
        if w.max(h) > RETINA_LONG as usize || w == 0 || h == 0 {
            return Ok(Vec::new());
        }
        let (pw, ph) = (w.div_ceil(32) * 32, h.div_ceil(32) * 32);
        let plane = pw * ph;
        let mut input = vec![0f32; 3 * plane];
        for y in 0..h {
            for (x, px) in img.data[y * w * 3..(y + 1) * w * 3].chunks_exact(3).enumerate() {
                let i = y * pw + x;
                input[i] = px[2] as f32 - RETINA_MEAN_BGR[0];
                input[plane + i] = px[1] as f32 - RETINA_MEAN_BGR[1];
                input[2 * plane + i] = px[0] as f32 - RETINA_MEAN_BGR[2];
            }
        }
        let priors = retina_priors(ph, pw);
        let n = priors.len();
        self.retina.with(|s| {
            let t = TensorRef::from_array_view(([1usize, 3, ph, pw], &input[..]))
                .map_err(|e| MlError::Runtime(format!("retinaface input: {e}")))?;
            let out = s.run(ort::inputs![t]).map_err(|e| MlError::Runtime(format!("retinaface: {e}")))?;
            let loc = output_f32(&out, "loc", 4 * n)?;
            let conf = output_f32(&out, "conf", 2 * n)?;
            let lm = output_f32(&out, "landms", 10 * n)?;
            Ok(retina_decode(&priors, loc, conf, lm, pw as f32, ph as f32))
        })
    }
}

static CHECK_JSON: &str = include_str!("../assets/maximum-check.json");

/// Synthetic input of the sanity check (same formula as the script that
/// computed the references): `scale·sin(0.05·(x+1)·(c+1))·cos(0.07·(y+1))`.
fn pattern(c: usize, h: usize, w: usize, scale: f32) -> Vec<f32> {
    let mut v = vec![0f32; c * h * w];
    for k in 0..c {
        for y in 0..h {
            for x in 0..w {
                v[(k * h + y) * w + x] =
                    scale * (0.05 * (x as f32 + 1.0) * (k as f32 + 1.0)).sin() * (0.07 * (y as f32 + 1.0)).cos();
            }
        }
    }
    v
}

/// Centred cosine (Pearson correlation) of two vectors.
pub(crate) fn correlation(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (ma, mb) = (a.iter().sum::<f32>() / a.len() as f32, b.iter().sum::<f32>() / b.len() as f32);
    let (mut ab, mut aa, mut bb) = (0f32, 0f32, 0f32);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (x - ma, y - mb);
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    let d = (aa * bb).sqrt();
    if d > 0.0 && d.is_finite() {
        ab / d
    } else {
        0.0
    }
}

fn expect_close(id: &str, got: &[f32], reference: &[f32]) -> Result<()> {
    let c = correlation(got, reference);
    log::info!("cull-ml: maximum pack check {id}: correlation {c:.5}");
    if c >= 0.99 {
        Ok(())
    } else {
        Err(MlError::Runtime(format!("{id}: wrong results (correlation {c:.3} with the reference)")))
    }
}

/// CLS row of DINOv2's `last_hidden_state` for a `[1, 3, 224, 224]` tensor.
fn dino_raw(s: &mut Session, input: &[f32]) -> Result<Vec<f32>> {
    let t = TensorRef::from_array_view(([1usize, 3, DINO_SIZE, DINO_SIZE], input))
        .map_err(|e| MlError::Runtime(format!("dinov2 input: {e}")))?;
    let out = s.run(ort::inputs![t]).map_err(|e| MlError::Runtime(format!("dinov2: {e}")))?;
    Ok(output_f32(&out, "last_hidden_state", DINO_DIM)?[..DINO_DIM].to_vec())
}

/// `[1, 3, H, W]` planar RGB `(v / 255 − mean) / std`.
fn normalized_chw(img: RgbView<'_>, mean: [f32; 3], std: [f32; 3]) -> Vec<f32> {
    let plane = (img.w * img.h) as usize;
    let mut out = vec![0f32; 3 * plane];
    for (i, px) in img.data.chunks_exact(3).take(plane).enumerate() {
        for c in 0..3 {
            out[c * plane + i] = (px[c] as f32 / 255.0 - mean[c]) / std[c];
        }
    }
    out
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// RF-DETR post-processing (no NMS: one query per object): every query's
/// best person/animal score above `RFDETR_SCORE`; the instance with the
/// largest mask (logit > 0) of at least `SUBJECT_MIN_AREA` of the frame.
fn largest_instance(labels: &[f32], masks: &[f32]) -> Option<SubjectObservation> {
    let mut best: Option<(usize, SubjectKind, f32, usize)> = None; // query, kind, score, pixels
    let side = RFDETR_MASK * RFDETR_MASK;
    for q in 0..RFDETR_QUERIES {
        let row = &labels[q * RFDETR_CLASSES..(q + 1) * RFDETR_CLASSES];
        let person = sigmoid(row[COCO_PERSON]);
        let animal = COCO_ANIMALS.map(|c| sigmoid(row[c])).fold(0f32, f32::max);
        let (kind, score) =
            if person >= animal { (SubjectKind::Person, person) } else { (SubjectKind::Animal, animal) };
        if score.is_nan() || score < RFDETR_SCORE {
            continue;
        }
        let pixels = masks[q * side..(q + 1) * side].iter().filter(|&&v| v > 0.0).count();
        if best.is_none_or(|b| pixels > b.3) {
            best = Some((q, kind, score, pixels));
        }
    }
    let (q, kind, score, pixels) = best?;
    let area_ratio = pixels as f32 / side as f32;
    if area_ratio < SUBJECT_MIN_AREA {
        return None;
    }
    let data: Vec<bool> = masks[q * side..(q + 1) * side].iter().map(|&v| v > 0.0).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (RFDETR_MASK, RFDETR_MASK, 0, 0);
    for (i, &on) in data.iter().enumerate() {
        if on {
            let (x, y) = (i % RFDETR_MASK, i / RFDETR_MASK);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    let n = RFDETR_MASK as f32;
    Some(SubjectObservation {
        kind,
        score,
        bbox: NormRect { x: x0 as f32 / n, y: y0 as f32 / n, w: (x1 - x0) as f32 / n, h: (y1 - y0) as f32 / n },
        area_ratio,
        sharpness: None,
        ev_over: None,
        mask: Some(SubjectMask { width: RFDETR_MASK as u32, height: RFDETR_MASK as u32, data }),
    })
}

/// RetinaFace anchors `(cx, cy, w, h)` normalised by the input size, in
/// the network's order: stride, row, column, size.
fn retina_priors(h: usize, w: usize) -> Vec<[f32; 4]> {
    let mut out = Vec::new();
    for (s, sizes) in RETINA_STEPS.iter().zip(RETINA_SIZES) {
        let (fh, fw) = (h.div_ceil(*s), w.div_ceil(*s));
        for i in 0..fh {
            for j in 0..fw {
                for m in sizes {
                    out.push([
                        (j as f32 + 0.5) * *s as f32 / w as f32,
                        (i as f32 + 0.5) * *s as f32 / h as f32,
                        m / w as f32,
                        m / h as f32,
                    ]);
                }
            }
        }
    }
    out
}

/// Boxes and landmarks (input pixels) of the faces above `RETINA_SCORE`,
/// after NMS. Variances 0.1 / 0.2; the 5 landmarks are in YuNet's order.
fn retina_decode(priors: &[[f32; 4]], loc: &[f32], conf: &[f32], lm: &[f32], w: f32, h: f32) -> Vec<Face> {
    let mut faces = Vec::new();
    for (k, p) in priors.iter().enumerate() {
        let score = conf[2 * k + 1];
        if score.is_nan() || score <= RETINA_SCORE {
            continue;
        }
        let l = &loc[4 * k..4 * k + 4];
        let cx = p[0] + l[0] * 0.1 * p[2];
        let cy = p[1] + l[1] * 0.1 * p[3];
        let bw = p[2] * (l[2] * 0.2).clamp(-10.0, 10.0).exp();
        let bh = p[3] * (l[3] * 0.2).clamp(-10.0, 10.0).exp();
        let mut kps = [[0f32; 2]; 5];
        for (j, kp) in kps.iter_mut().enumerate() {
            *kp = [(p[0] + lm[10 * k + 2 * j] * 0.1 * p[2]) * w, (p[1] + lm[10 * k + 2 * j + 1] * 0.1 * p[3]) * h];
        }
        let face = Face { score, x: (cx - bw / 2.0) * w, y: (cy - bh / 2.0) * h, w: bw * w, h: bh * h, kps };
        if [face.x, face.y, face.w, face.h].iter().all(|v| v.is_finite()) && kps.iter().flatten().all(|v| v.is_finite())
        {
            faces.push(face);
        }
    }
    nms(faces, RETINA_NMS)
}

fn scale_face(mut f: Face, sx: f32, sy: f32) -> Face {
    f.x *= sx;
    f.y *= sy;
    f.w *= sx;
    f.h *= sy;
    for k in &mut f.kps {
        k[0] *= sx;
        k[1] *= sy;
    }
    f
}

fn iou(a: &Face, b: &Face) -> f32 {
    let ix = ((a.x + a.w).min(b.x + b.w) - a.x.max(b.x)).max(0.0);
    let iy = ((a.y + a.h).min(b.y + b.h) - a.y.max(b.y)).max(0.0);
    let inter = ix * iy;
    let union = a.w * a.h + b.w * b.h - inter;
    if union > 0.0 {
        inter / union
    } else {
        0.0
    }
}

/// The centre of `b` lies inside `a` (detectors box a face differently:
/// YuNet's box is larger than RetinaFace's).
fn centre_inside(a: &Face, b: &Face) -> bool {
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    cx >= a.x && cx <= a.x + a.w && cy >= a.y && cy <= a.y + a.h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priors_match_the_network_layout() {
        // 64x96 input: 2 sizes x (8x12 + 4x6 + 2x3) anchors.
        let p = retina_priors(64, 96);
        assert_eq!(p.len(), 2 * (8 * 12 + 4 * 6 + 2 * 3));
        assert_eq!(p[0], [4.0 / 96.0, 4.0 / 64.0, 16.0 / 96.0, 16.0 / 64.0]);
        assert_eq!(p[1][2], 32.0 / 96.0);
        assert_eq!(p[2][0], 12.0 / 96.0, "next column");
    }

    #[test]
    fn decoding_a_single_anchor() {
        let priors = retina_priors(64, 96);
        let n = priors.len();
        let (mut loc, mut conf, lm) = (vec![0f32; 4 * n], vec![0f32; 2 * n], vec![0f32; 10 * n]);
        conf[2 * 5 + 1] = 0.97;
        loc[4 * 5 + 2] = 1.0; // wider: 16 x exp(0.2)
        let faces = retina_decode(&priors, &loc, &conf, &lm, 96.0, 64.0);
        assert_eq!(faces.len(), 1);
        let f = faces[0];
        let p = priors[5];
        assert!((f.x + f.w / 2.0 - p[0] * 96.0).abs() < 1e-3);
        assert!((f.w - p[2] * 96.0 * 0.2f32.exp()).abs() < 1e-3);
        assert!((f.h - p[3] * 64.0).abs() < 1e-3);
        assert_eq!(f.kps[0], [p[0] * 96.0, p[1] * 64.0]);
    }

    #[test]
    fn the_largest_person_or_animal_is_the_subject() {
        let side = RFDETR_MASK * RFDETR_MASK;
        let mut labels = vec![-10f32; RFDETR_QUERIES * RFDETR_CLASSES];
        let mut masks = vec![-1f32; RFDETR_QUERIES * side];
        // Query 3: a person over a 20x30 block; query 7: a dog, larger but
        // below the score; query 9: a tie (another class only).
        labels[3 * RFDETR_CLASSES + COCO_PERSON] = 3.0;
        for y in 10..40 {
            for x in 20..40 {
                masks[3 * side + y * RFDETR_MASK + x] = 1.0;
            }
        }
        labels[7 * RFDETR_CLASSES + 18] = -0.5;
        masks[7 * side..8 * side].fill(1.0);
        labels[9 * RFDETR_CLASSES + 40] = 5.0;
        masks[9 * side..10 * side].fill(1.0);
        let s = largest_instance(&labels, &masks).expect("subject");
        assert_eq!(s.kind, SubjectKind::Person);
        assert!((s.area_ratio - 600.0 / side as f32).abs() < 1e-6);
        let n = RFDETR_MASK as f32;
        assert_eq!(s.bbox, NormRect { x: 20.0 / n, y: 10.0 / n, w: 20.0 / n, h: 30.0 / n });
        let m = s.mask.unwrap();
        assert!(m.contains(30.5 / n, 20.5 / n) && !m.contains(0.5 / n, 0.5 / n));
        // Too small a region is not a subject.
        let mut tiny = vec![-1f32; RFDETR_QUERIES * side];
        tiny[3 * side] = 1.0;
        assert!(largest_instance(&labels, &tiny).is_none());
    }

    #[test]
    fn correlation_and_pattern() {
        assert!((correlation(&[1.0, 2.0, 3.0], &[2.0, 4.0, 6.5]) - 1.0).abs() < 0.01);
        assert!(correlation(&[1.0, 2.0, 3.0], &[3.0, 2.0, 1.0]) < -0.99);
        assert_eq!(correlation(&[1.0], &[]), 0.0);
        let p = pattern(3, 2, 2, 1.5);
        assert!((p[0] - 1.5 * 0.05f32.sin() * 0.07f32.cos()).abs() < 1e-6);
    }
}
