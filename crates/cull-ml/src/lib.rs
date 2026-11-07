//! Neural networks of Pelli Instant, run locally with ONNX Runtime.
//!
//! Models (all with licenses allowing commercial use, see
//! `models/manifest.json` and this crate's README):
//! - faces: YuNet 2023mar (OpenCV Zoo, MIT) — boxes + 5 landmarks;
//! - face mesh: MediaPipe Face Mesh V2 (Google, Apache-2.0) landmarks on each
//!   face crop → eye aspect ratio → open probability, head pose, mouth
//!   opening, and whether it is a face at all (see `eyes`);
//! - CLIP ViT-B/32 image tower (OpenAI, MIT; int8 on CPU, fp16 on GPU/NPU) —
//!   embeddings used for similar-shot grouping and text search;
//! - CLIP ViT-B/32 text tower (fp16) for natural-language search;
//! - aesthetic head: LAION linear predictor on CLIP ViT-B/32 (MIT), compiled
//!   into the crate.
//!
//! Every model is optional at runtime except faces: a missing file degrades
//! the analysis (no aesthetic score…), it never fails the whole shoot.
//!
//! CONTRACT — these signatures are used by `cull-engine` and must stay
//! stable.

mod clip;
mod expression;
mod eyes;
mod faces;
mod imgproc;
mod manifest;
mod maximum;
mod onnx_patch;
mod runtime;

pub use cull_types::Precision;
pub use manifest::sha256_file;
pub use maximum::{maximum_pack_files, MaximumModels, MaximumText, PackFile};

pub use cull_types::HeadPose;
use cull_types::{FaceObservation, MlObservations, NormPoint, NormRect};
use image::RgbImage;
use imgproc::{fit_long_side, resize, RgbView};
use runtime::{build_first, compiled_accelerators, default_pool_size, ModelOpts, ModelSource, Provider, SessionPool};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Inter-ocular distance of the smallest frontal face the face mesh reads
/// (eye states, pose), in pixels of the most detailed image given to
/// [`ModelSet::analyze_with_detail`]: the mesh runs on face boxes at least
/// 2.4x this size (profiles included); smaller faces get `None`.
pub const EYE_STATE_MIN_IOD_PX: f32 = eyes::MIN_IOD_PX;

/// A face is "small" below this inter-ocular distance, in pixels of the
/// analysed image scaled to a 1440 px long edge: its eyes and focus are read
/// on the full-resolution detail image when the caller can provide one.
pub const DETAIL_IOD_PX: f32 = 40.0;
/// Detector confidence from which a small face asks for the detail image:
/// the confidence a face needs to be a subject (each detail decode costs a
/// second decode of the photo).
const DETAIL_MIN_CONFIDENCE: f32 = 0.8;
/// Smallest inter-ocular distance (px at 1440) worth a detail decode:
/// below, a face is a figure in the background (< 0.07% of the frame).
const DETAIL_MIN_IOD_PX: f32 = 12.0;
/// The detail image is at most this much larger than the analysed one (a
/// half-size decode of a 24-45 MP JPEG: 4x less memory than full size).
const DETAIL_MAX_GAIN: f32 = 2.5;
/// A group photo (this many confident faces with an inter-ocular distance
/// under `DETAIL_GROUP_IOD_PX` at 1440 px) asks for the detail image too.
const DETAIL_GROUP_FACES: usize = 3;
const DETAIL_GROUP_IOD_PX: f32 = 64.0;
/// Inter-ocular distance the detail image is asked to give the smallest of
/// the faces that asked for it (the mesh reads eyes reliably from ~40 px).
pub const DETAIL_TARGET_IOD_PX: f32 = 64.0;

#[derive(Debug, thiserror::Error)]
pub enum MlError {
    #[error("model file missing: {0}")]
    MissingModel(String),
    #[error("model checksum mismatch for {0}")]
    Checksum(String),
    #[error("onnx runtime error: {0}")]
    Runtime(String),
}

pub type Result<T> = std::result::Result<T, MlError>;

/// Hardware acceleration preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Acceleration {
    /// The fastest measured: DirectML on Windows, CUDA when available on
    /// Linux, the CPU otherwise. On Apple Silicon that is the CPU: CLIP int8
    /// on the CPU runs 3-4x faster than CoreML fp16 (M4: 60-80 photos/s
    /// against ~20, CoreML's dynamic-batch graph only partly reaches the
    /// Neural Engine and its 2 sessions serialise the workers).
    #[default]
    Auto,
    CpuOnly,
    /// The accelerator whenever one is compiled in, CoreML included
    /// (benchmarks).
    Accelerator,
}

#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    pub acceleration: Acceleration,
    /// Intra-op threads per session (the engine parallelises across photos;
    /// 1–2 is usually best).
    pub intra_threads: usize,
    /// Load the CLIP text tower too (natural-language search).
    pub text_search: bool,
    /// Directory of the checksum cache. `None` = the models directory; set
    /// it when the models live in a read-only or signed location (app
    /// bundle) so nothing is ever written next to them.
    pub checksum_cache_dir: Option<std::path::PathBuf>,
    /// Where accelerators keep the models they compiled (CoreML), so the
    /// next launch reuses them. `None` = recompiled at every launch.
    pub accelerator_cache_dir: Option<std::path::PathBuf>,
    /// Physical memory of the computer, when known: the Maximum pack keeps
    /// fewer sessions of its heavy models below 12 GB.
    pub memory_bytes: Option<u64>,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        RuntimeOptions {
            acceleration: Acceleration::Auto,
            intra_threads: 1,
            text_search: true,
            checksum_cache_dir: None,
            accelerator_cache_dir: None,
            memory_bytes: None,
        }
    }
}

/// Which optional capabilities are loaded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub faces: bool,
    pub eyes: bool,
    pub embeddings: bool,
    pub aesthetic: bool,
    pub text_search: bool,
}

/// All loaded models. `Send + Sync`: the engine calls `analyze` from several
/// rayon threads at once (each call checks a session of each model out of a
/// small pool, grown up to the number of CPU cores: in advance with
/// [`ModelSet::prepare`], or on demand).
pub struct ModelSet {
    faces: faces::FaceDetector,
    eyes: Option<eyes::EyeModel>,
    expression: Option<expression::ExpressionModel>,
    vision: Option<clip::ClipVision>,
    text: Option<clip::ClipText>,
    /// Tags of the face models (both precisions) and of the Standard
    /// embedding models.
    face_tags: Vec<String>,
    standard_tags: Vec<String>,
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ModelSet>();
};

/// Upper bound of concurrent GPU/NPU sessions (the device serialises work
/// anyway; two let CPU preprocessing overlap).
const ACCEL_POOL_SIZE: usize = 2;

fn guard<T>(what: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => Err(MlError::Runtime(format!("internal error in {what}"))),
    }
}

impl ModelSet {
    /// Load the models found in `models_dir` (file names from
    /// `models/manifest.json`), verifying SHA-256 checksums.
    ///
    /// The manifest is compiled into the crate (it need not be shipped).
    /// Missing optional models disable the matching capability; a missing
    /// YuNet file is `MissingModel`; a present but corrupt file is
    /// `Checksum`.
    pub fn load(models_dir: &Path, opts: &RuntimeOptions) -> Result<ModelSet> {
        guard("ModelSet::load", || Self::load_inner(models_dir, opts))
    }

    fn load_inner(dir: &Path, opts: &RuntimeOptions) -> Result<ModelSet> {
        manifest::set_cache_dir(opts.checksum_cache_dir.clone());
        let intra = opts.intra_threads.clamp(1, 64);
        let cpu_pool = default_pool_size();
        let mut tags: Vec<String> = Vec::new();
        let mut standard_tags: Vec<String> = Vec::new();

        for e in manifest::entries()?.iter().filter(|e| e.required) {
            if !dir.join(&e.file).is_file() {
                return Err(MlError::MissingModel(e.file.clone()));
            }
        }

        // Faces (required). Small: kept in memory, patched for dynamic H/W.
        let yunet = manifest::load_verified(dir, "yunet")?.ok_or_else(|| {
            MlError::MissingModel(manifest::entry("yunet").map(|e| e.file.clone()).unwrap_or_default())
        })?;
        let patched: Arc<[u8]> = onnx_patch::make_spatial_dims_dynamic(&yunet.bytes, "input")?.into();
        let (first, spec) =
            build_first("yunet", ModelSource::Memory(patched.clone()), Some(&patched), &ModelOpts::cpu(intra))?;
        let faces = faces::FaceDetector { pool: SessionPool::new(first, spec, None, cpu_pool) };
        tags.push(yunet.entry.tag.clone());
        drop(yunet);

        // Eye state (optional).
        let eyes = match manifest::load_verified(dir, "face-mesh")? {
            Some(m) => {
                let bytes: Arc<[u8]> = m.bytes.into();
                let (first, spec) =
                    build_first("face-mesh", ModelSource::Memory(bytes.clone()), Some(&bytes), &ModelOpts::cpu(intra))?;
                tags.push(format!("{}-{}", m.entry.tag, eyes::ALGO_TAG));
                Some(eyes::EyeModel { pool: SessionPool::new(first, spec, None, cpu_pool) })
            }
            None => {
                log::warn!("cull-ml: face mesh model missing: no eye state");
                None
            }
        };

        // Expression (optional, needs the mesh): blink / squint / smile.
        // Always on the CPU (CoreML splits it into 9 partitions, slower).
        let expression = match (&eyes, manifest::load_verified(dir, "face-blendshapes")?) {
            (Some(_), Some(m)) => {
                let bytes: Arc<[u8]> = m.bytes.into();
                let src = ModelSource::Memory(bytes.clone());
                let (first, spec) = build_first("face-blendshapes", src, Some(&bytes), &ModelOpts::cpu(intra))?;
                tags.push(format!("{}-{}", m.entry.tag, expression::ALGO_TAG));
                Some(expression::ExpressionModel { pool: SessionPool::new(first, spec, None, cpu_pool) })
            }
            (Some(_), None) => {
                log::warn!("cull-ml: face blendshapes model missing: eye state from the eye aspect ratio");
                None
            }
            (None, _) => None,
        };

        // CLIP image tower (optional): fp16 on an accelerator when one is
        // compiled in, works and is faster (not CoreML: see `Acceleration`),
        // otherwise int8 on the CPU.
        let accels: Vec<Provider> = match opts.acceleration {
            Acceleration::CpuOnly => Vec::new(),
            Acceleration::Accelerator => compiled_accelerators(),
            Acceleration::Auto => compiled_accelerators().into_iter().filter(|p| *p != Provider::CoreMl).collect(),
        };
        let accel_cache = opts.accelerator_cache_dir.as_deref();
        let mut vision = None;
        if !accels.is_empty() {
            if let Some(m) = manifest::load_verified(dir, "clip-vision-fp16")? {
                let src = ModelSource::File(m.path.clone());
                let o = ModelOpts {
                    candidates: &accels,
                    cpu_fallback: false,
                    cache_dir: accel_cache,
                    ..ModelOpts::cpu(intra)
                };
                match build_first("clip-vision-fp16", src, Some(&m.bytes), &o) {
                    Ok((first, spec)) => {
                        standard_tags.push(format!("{}-{}", m.entry.tag, clip::PREPROCESS_TAG));
                        vision = Some(clip::ClipVision {
                            pool: SessionPool::new(first, spec, None, ACCEL_POOL_SIZE),
                            dim: clip::EMBED_DIM,
                        });
                    }
                    Err(e) => log::warn!("cull-ml: no accelerator for CLIP ({e}), using the CPU"),
                }
            }
        }
        if vision.is_none() {
            let m = match manifest::load_verified(dir, "clip-vision")? {
                Some(m) => Some(m),
                None => manifest::load_verified(dir, "clip-vision-fp16")?,
            };
            match m {
                Some(m) => {
                    let prepacked = ort::session::builder::PrepackedWeights::new();
                    let src = ModelSource::File(m.path.clone());
                    let o = ModelOpts { prepacked: Some(&prepacked), ..ModelOpts::cpu(intra) };
                    let (first, spec) = build_first("clip-vision", src, Some(&m.bytes), &o)?;
                    standard_tags.push(format!("{}-{}", m.entry.tag, clip::PREPROCESS_TAG));
                    vision = Some(clip::ClipVision {
                        pool: SessionPool::new(first, spec, Some(prepacked), cpu_pool),
                        dim: clip::EMBED_DIM,
                    });
                }
                None => log::warn!("cull-ml: CLIP image model missing: no embeddings, no aesthetic score"),
            }
        }
        if vision.is_some() {
            standard_tags.push("laion-linear-b32".to_string());
        }

        // CLIP text tower (optional, only for search; not part of models_id).
        let mut text = None;
        if opts.text_search {
            if let Some(m) = manifest::load_verified(dir, "clip-text")? {
                // Interactive and rare: give it a few threads.
                let threads = intra.max(default_pool_size().min(4));
                let src = ModelSource::File(m.path.clone());
                let (session, _) = build_first("clip-text", src, Some(&m.bytes), &ModelOpts::cpu(threads))?;
                text = Some(clip::ClipText {
                    session: Mutex::new(session),
                    tokenizer: instant_clip_tokenizer::Tokenizer::new(),
                    dim: clip::EMBED_DIM,
                });
            } else {
                log::warn!("cull-ml: CLIP text model missing: no text search");
            }
        }

        Ok(ModelSet { faces, eyes, expression, vision, text, face_tags: tags, standard_tags })
    }

    pub fn capabilities(&self) -> Capabilities {
        Capabilities {
            faces: true,
            eyes: self.eyes.is_some(),
            embeddings: self.vision.is_some(),
            aesthetic: self.vision.is_some(),
            text_search: self.text.is_some(),
        }
    }

    /// Create the sessions `workers` concurrent [`ModelSet::analyze`] calls
    /// need (one per worker and model, within each pool's maximum), in
    /// parallel, so the first photos of an analysis do not wait for them.
    /// Sessions are kept for the life of the `ModelSet`: a second call is
    /// free. Thread-safe; failures only leave the pools to grow on demand.
    pub fn prepare(&self, workers: usize) {
        self.prepare_for(workers, Precision::Standard)
    }

    /// [`ModelSet::prepare`] for analyses in `precision`: in Maximum the
    /// CLIP ViT-B/32 sessions are not needed (~140 MB each).
    pub fn prepare_for(&self, workers: usize, precision: Precision) {
        let workers = workers.max(1);
        std::thread::scope(|s| {
            s.spawn(|| self.faces.pool.warm(workers));
            if let Some(e) = &self.eyes {
                s.spawn(|| e.pool.warm(workers));
            }
            if let Some(x) = &self.expression {
                s.spawn(|| x.pool.warm(workers));
            }
            if let (Some(v), Precision::Standard) = (&self.vision, precision) {
                s.spawn(|| v.pool.warm(workers));
            }
        });
    }

    /// Execution provider running the heavy model (CLIP image tower):
    /// `"CoreML"`, `"DirectML"`, `"CUDA"` or `"CPU"`. The small models
    /// (faces, eye mesh) always run on the CPU.
    pub fn accelerator(&self) -> String {
        self.vision.as_ref().map(|v| v.pool.provider()).unwrap_or(Provider::Cpu).name().to_string()
    }

    /// Faces (with the face mesh reading: eye-open probabilities, presence,
    /// pose; `sharpness`, skin and body hash left `None`), embedding and
    /// aesthetic score of one upright photo (any size; resized internally).
    /// Target: < 60 ms per photo on one laptop core with int8 CLIP.
    ///
    /// Faces are sorted by decreasing size (main subject first). The mesh
    /// runs on `img` for the 24 largest faces large enough (see
    /// [`EYE_STATE_MIN_IOD_PX`]); others get `None`.
    pub fn analyze(&self, img: &RgbImage) -> Result<MlObservations> {
        guard("analyze", || self.analyze_inner(img, &|_| None, None))
    }

    /// [`ModelSet::analyze`], reading small faces on a more detailed image:
    /// when the photo has a face confident enough to be a subject (≥ 0.8)
    /// whose inter-ocular distance is between 12 and [`DETAIL_IOD_PX`] px at
    /// 1440 px, or a group of 3 such faces under 64 px, `detail(long_edge)`
    /// is called (at most once) for the same photo, upright, with a long
    /// edge of at least `long_edge` px when the source allows it (enough for
    /// the smallest of those faces to have [`DETAIL_TARGET_IOD_PX`] between
    /// the eyes, at most 2.5x the analysed image; the RAW's full-size JPEG,
    /// the camera JPEG itself), and the face mesh reads every face on it. A
    /// `detail` image of a different aspect ratio, or not larger than `img`,
    /// is ignored.
    pub fn analyze_with_detail<'d>(
        &self,
        img: &RgbImage,
        detail: &dyn Fn(u32) -> Option<&'d RgbImage>,
    ) -> Result<MlObservations> {
        guard("analyze", || self.analyze_inner(img, detail, None))
    }

    /// [`ModelSet::analyze_with_detail`] in Maximum precision: RetinaFace
    /// adds the faces YuNet missed on group photos, DINOv2 gives the
    /// grouping `embedding`, CLIP ViT-L/14 the `search_embedding` and the
    /// LAION V2 aesthetic score, and RF-DETR the `subject` region of photos
    /// without a usable face (see [`MaximumModels`]).
    pub fn analyze_maximum<'d>(
        &self,
        max: &MaximumModels,
        img: &RgbImage,
        detail: &dyn Fn(u32) -> Option<&'d RgbImage>,
    ) -> Result<MlObservations> {
        guard("analyze", || self.analyze_inner(img, detail, Some(max)))
    }

    fn analyze_inner<'d>(
        &self,
        img: &RgbImage,
        detail: &dyn Fn(u32) -> Option<&'d RgbImage>,
        max: Option<&MaximumModels>,
    ) -> Result<MlObservations> {
        let models = match max {
            Some(m) => self.models_id_maximum(m),
            None => self.models_id(),
        };
        let mut obs = MlObservations { models, ..Default::default() };
        let Some(full) = RgbView::new(img.width(), img.height(), img.as_raw()) else {
            return Ok(obs); // empty image: nothing to see
        };
        // Working copy, long side ≤ 1280: fine face pass, CLIP, big face crops.
        let (ww, wh) = fit_long_side(full.w, full.h, faces::FINE_LONG);
        let work_buf;
        let work = if (ww, wh) == (full.w, full.h) {
            full
        } else {
            use fast_image_resize::{FilterType, ResizeAlg};
            work_buf = resize(full, ww, wh, ResizeAlg::Convolution(FilterType::Box))?;
            work_buf.view()
        };

        let mut detected = self.faces.detect(full, work)?;
        if let Some(m) = max {
            let t = std::time::Instant::now();
            let more = m.more_faces(full, work, &detected)?;
            if more.is_some() {
                log::trace!("cull-ml: retinaface {:?}", t.elapsed());
            }
            if let Some(more) = more {
                detected.extend(more);
                detected.sort_by(|a, b| (b.w * b.h).total_cmp(&(a.w * a.h)));
            }
        }
        let mut readings = match &self.eyes {
            Some(e) if !detected.is_empty() => {
                let wanted = detail_long_edge(&detected, full.w.max(full.h));
                let fine = wanted.and_then(detail).and_then(|d| detail_view(d, full));
                let (src, scale) = fine.unwrap_or((full, 1.0));
                e.read(src, scale, work, work.w as f32 / full.w as f32, &detected)?
            }
            _ => vec![None; detected.len()],
        };
        if let Some(x) = &self.expression {
            self.read_expressions(x, &mut readings)?;
        }
        let (fw, fh) = (full.w as f32, full.h as f32);
        let norm = |p: [f32; 2]| NormPoint { x: (p[0] / fw).clamp(0.0, 1.0), y: (p[1] / fh).clamp(0.0, 1.0) };
        obs.faces = detected
            .iter()
            .zip(readings)
            .map(|(f, reading)| {
                let r = reading.as_ref();
                let x0 = (f.x / fw).clamp(0.0, 1.0);
                let y0 = (f.y / fh).clamp(0.0, 1.0);
                let x1 = ((f.x + f.w) / fw).clamp(0.0, 1.0);
                let y1 = ((f.y + f.h) / fh).clamp(0.0, 1.0);
                let bbox = NormRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
                FaceObservation {
                    bbox,
                    confidence: f.score.clamp(0.0, 1.0),
                    landmarks: f.kps.map(norm),
                    right_eye_open: r.and_then(|r| r.right_open),
                    left_eye_open: r.and_then(|r| r.left_open),
                    expression: r.and_then(|r| r.expression),
                    sharpness: None,
                    area_ratio: bbox.w * bbox.h,
                    mesh_presence: r.map(|r| r.presence),
                    pose: r.and_then(|r| r.pose),
                    skin_luma: None,
                    skin_clipped: None,
                    body_hash: None,
                }
            })
            .collect();

        match max {
            Some(m) => {
                let t = std::time::Instant::now();
                obs.embedding = Some(m.dino_embed(work)?);
                let t_dino = t.elapsed();
                let (e, a) = m.clip_embed(work)?;
                let t_clip = t.elapsed() - t_dino;
                obs.search_embedding = Some(e);
                obs.aesthetic = a;
                if !obs.faces.iter().any(usable_face) {
                    obs.subject = m.subject(work)?;
                }
                log::trace!(
                    "cull-ml: dinov2 {t_dino:?} clip-l14 {t_clip:?} rfdetr {:?}",
                    t.elapsed() - t_dino - t_clip
                );
            }
            None => {
                if let Some(v) = &self.vision {
                    let e = v.embed(work)?;
                    obs.aesthetic = clip::aesthetic(&e);
                    obs.embedding = Some(e);
                }
            }
        }
        Ok(obs)
    }

    /// Blendshapes of every face the mesh read: `expression`, and the eye
    /// state from them (an eye the mesh found hidden stays `None`).
    fn read_expressions(
        &self,
        x: &expression::ExpressionModel,
        readings: &mut [Option<eyes::MeshReading>],
    ) -> Result<()> {
        let idx: Vec<usize> = (0..readings.len())
            .filter(|&i| readings[i].as_ref().is_some_and(|r| r.pose.is_some() && !r.landmarks.is_empty()))
            .collect();
        let meshes: Vec<&[f32]> =
            idx.iter().filter_map(|&i| readings[i].as_ref().map(|r| r.landmarks.as_slice())).collect();
        let scores = x.run(&meshes)?;
        for (&i, s) in idx.iter().zip(scores) {
            let (Some(r), Some(s)) = (readings[i].as_mut(), s) else { continue };
            let Some(e) = expression::expression_of(&s) else { continue };
            let ear = [r.right_ear.map(eyes::open_logit), r.left_ear.map(eyes::open_logit)];
            let [right, left] = expression::open_probability(&e, ear);
            r.right_open = right.or(r.right_open);
            r.left_open = left.or(r.left_open);
            r.expression = Some(e);
        }
        Ok(())
    }

    /// L2-normalised text embedding (CLIP text tower) for natural-language
    /// search ("mariée avec bouquet", "first dance"). Queries in French and
    /// other languages work poorly with English CLIP: the caller may
    /// translate; this function embeds the text as given.
    pub fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        match &self.text {
            Some(t) => guard("embed_text", || t.embed(text)),
            None => Err(MlError::MissingModel(
                manifest::entry("clip-text").map(|e| e.file.clone()).unwrap_or_else(|_| "clip text model".into()),
            )),
        }
    }

    /// Identifier of the loaded model versions (and of the algorithms reading
    /// them), stored with each analysis (Standard precision).
    pub fn models_id(&self) -> String {
        self.face_tags.iter().chain(&self.standard_tags).cloned().collect::<Vec<_>>().join("+")
    }

    /// Identifier of an analysis in Maximum precision: the face models and
    /// the pack's (switching precision re-measures the photos).
    pub fn models_id_maximum(&self, max: &MaximumModels) -> String {
        self.face_tags.iter().chain(max.tags()).cloned().collect::<Vec<_>>().join("+")
    }
}

/// `true` for a `models_id` of an analysis in Maximum precision.
pub fn is_maximum_models_id(models_id: &str) -> bool {
    models_id.split('+').any(|t| t.starts_with("clip-l14"))
}

/// A face that can be the subject of the photo (as `cull-analysis` decides
/// the main face): confident, a face for the mesh, big enough.
fn usable_face(f: &FaceObservation) -> bool {
    f.confidence >= 0.8 && f.mesh_presence.is_none_or(|p| p >= eyes::MIN_PRESENCE) && f.area_ratio >= 0.0015
}

/// Long edge of the detail image the faces of this photo need, `None` when
/// they need none (see [`ModelSet::analyze_with_detail`]). `long` is the
/// long side of the image the faces were detected on.
fn detail_long_edge(faces: &[faces::Face], long: u32) -> Option<u32> {
    let to_1440 = 1440.0 / long.max(1) as f32;
    let confident = || faces.iter().take(eyes::MAX_FACES).filter(|f| f.score >= DETAIL_MIN_CONFIDENCE);
    let small: Vec<f32> =
        confident().map(|f| f.iod()).filter(|&d| (DETAIL_MIN_IOD_PX..DETAIL_IOD_PX).contains(&(d * to_1440))).collect();
    let group: Vec<f32> = confident().map(|f| f.iod()).filter(|&d| d * to_1440 < DETAIL_GROUP_IOD_PX).collect();
    let needing = if !small.is_empty() {
        small
    } else if group.len() >= DETAIL_GROUP_FACES {
        group
    } else {
        return None;
    };
    let smallest = needing.into_iter().fold(f32::INFINITY, f32::min).max(1.0);
    let gain = (DETAIL_TARGET_IOD_PX / smallest).clamp(1.25, DETAIL_MAX_GAIN);
    Some((long as f32 * gain).round() as u32)
}

/// `detail` as a view with its scale relative to `full`, when it is the same
/// picture (aspect ratio within 1%) and larger.
fn detail_view<'d>(detail: &'d RgbImage, full: RgbView<'_>) -> Option<(RgbView<'d>, f32)> {
    let v = RgbView::new(detail.width(), detail.height(), detail.as_raw())?;
    let scale = v.w as f32 / full.w as f32;
    let same_shape = ((v.h as f32 / full.h as f32) / scale - 1.0).abs() < 0.01;
    (same_shape && scale > 1.05).then_some((v, scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(score: f32, iod: f32) -> faces::Face {
        faces::Face {
            score,
            x: 0.0,
            y: 0.0,
            w: 2.5 * iod,
            h: 3.0 * iod,
            kps: [[0.0, 0.0], [iod, 0.0], [0.0; 2], [0.0; 2], [0.0; 2]],
        }
    }

    #[test]
    fn detail_is_asked_for_small_confident_faces_and_groups() {
        // 1620 px preview: IOD 32 px → 28.4 at 1440: small, needs 2x.
        assert_eq!(detail_long_edge(&[face(0.9, 32.0)], 1620), Some(3240));
        assert_eq!(detail_long_edge(&[face(0.9, 80.0)], 1620), None, "a portrait reads fine");
        assert_eq!(detail_long_edge(&[face(0.75, 30.0)], 1620), None, "not a subject: no decode");
        assert_eq!(detail_long_edge(&[face(0.9, 8.0)], 1620), None, "a figure in the background");
        let group = [face(0.9, 55.0), face(0.9, 55.0), face(0.85, 50.0)];
        assert_eq!(detail_long_edge(&group, 1620), Some(2074), "group: at least 1.25x");
        assert_eq!(detail_long_edge(&[face(0.9, 15.0)], 1620), Some(4050), "capped at 2.5x");
        assert_eq!(detail_long_edge(&[], 1620), None);
    }

    #[test]
    fn detail_must_be_the_same_picture_and_larger() {
        let full = RgbImage::new(160, 100);
        let fv = RgbView::new(160, 100, full.as_raw()).unwrap();
        let big = RgbImage::new(640, 400);
        assert_eq!(detail_view(&big, fv).map(|(v, s)| (v.w, s)), Some((640, 4.0)));
        assert!(detail_view(&RgbImage::new(640, 480), fv).is_none(), "other aspect ratio");
        assert!(detail_view(&RgbImage::new(160, 100), fv).is_none(), "not larger");
    }
}
