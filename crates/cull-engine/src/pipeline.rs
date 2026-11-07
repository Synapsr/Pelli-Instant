//! The analysis pipeline.
//!
//! ```text
//!   rayon workers (N)                         job thread
//!   ┌──────────────────────────────┐          ┌──────────────────────────┐
//!   │ decode preview (DCT scaled)  │  rows    │ batch-save to SQLite      │
//!   │ thumbnail → cache            │ ───────► │ provisional decisions (1s)│
//!   │ metrics + ML                 │ channel  │ events: photo, progress   │
//!   └──────────────────────────────┘          └──────────────────────────┘
//! ```
//!
//! Re-running is incremental: photos whose file, and models, did not change
//! are skipped; a missing thumbnail only triggers a decode. Photos analysed
//! for the first time are counted on the Pelli monthly quota ([`crate::usage`]).
//!
//! Workers run at the priority of the analysis speed setting
//! ([`crate::settings::AppSettings::background`]): all cores at
//! user-initiated priority, or a few threads at background priority (macOS
//! efficiency cores, below-normal priority on Windows) so the computer stays
//! fluid. The ONNX sessions are created when the models load and kept
//! between analyses.
//!
//! Small faces (a group, a figure far away) are read on a more detailed
//! image decoded on demand ([`load_detail`]): the RAW's full-size JPEG, a
//! JPEG twin, the camera JPEG itself.

use crate::store::{AnalysisRow, ProjectRow};
use crate::view::{reason_key, AnalysisSummary, DecisionCounts};
use crate::{now_ms, Engine, EngineError, ProjectState, Result, ANALYSIS_EDGE, THUMB_EDGE};
use cull_analysis::{EmbeddingSpace, PhotoFacts};
use cull_types::{
    final_decision, ExifInfo, FaceObservation, FileKind, MlObservations, NormRect, PhotoAsset, Precision, Progress,
    SourceFile, Stage, SubjectObservation, TechnicalMetrics,
};
use image::RgbImage;
use parking_lot::Mutex;
use std::cell::OnceCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Eye-open probability below which an eye counts as closed.
pub const EYE_OPEN_THRESHOLD: f32 = 0.5;
/// Models identifier of analyses made without the AI models.
const METRICS_ONLY: &str = "metrics-only";

/// Owned copy of the facts the decision step needs.
pub(crate) struct OwnedFacts {
    pub(crate) id: String,
    exif: ExifInfo,
    metrics: TechnicalMetrics,
    faces: Vec<FaceObservation>,
    embedding: Option<Vec<f32>>,
    space: EmbeddingSpace,
    aesthetic: Option<f32>,
    subject: Option<SubjectObservation>,
}

impl OwnedFacts {
    pub(crate) fn from_row(row: &AnalysisRow) -> Option<OwnedFacts> {
        Some(OwnedFacts {
            id: row.photo_id.clone(),
            exif: row.exif.clone().unwrap_or_default(),
            metrics: row.metrics?,
            faces: row.faces.clone(),
            embedding: row.embedding.clone(),
            space: if cull_ml::is_maximum_models_id(&row.models) {
                EmbeddingSpace::DinoV2
            } else {
                EmbeddingSpace::ClipB32
            },
            aesthetic: row.aesthetic,
            subject: row.subject.clone(),
        })
    }

    pub(crate) fn as_facts(&self) -> PhotoFacts<'_> {
        PhotoFacts {
            id: &self.id,
            exif: &self.exif,
            metrics: &self.metrics,
            faces: &self.faces,
            embedding: self.embedding.as_deref(),
            embedding_space: self.space,
            aesthetic: self.aesthetic,
            subject: self.subject.as_ref(),
        }
    }
}

/// Write a file atomically (temp + rename) creating parent dirs.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| EngineError::io(parent, e))?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| EngineError::io(&tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| EngineError::io(path, e))
}

/// Region used to judge the focus of a face whose eyes cannot be placed: the
/// central part of the box (eyes, nose), without hair and background that
/// may legitimately be soft.
pub fn face_focus_region(f: &FaceObservation) -> NormRect {
    let b = f.bbox;
    NormRect { x: b.x + b.w * 0.15, y: b.y + b.h * 0.15, w: b.w * 0.7, h: b.h * 0.55 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Need {
    Full,
    ThumbOnly,
}

/// What an analysis with `models_id` has to do: photos never analysed, or
/// whose file or models changed, or that failed, need a full analysis; an
/// up-to-date one only needs its thumbnail when the cache lost it.
fn work_to_do(
    assets: &[PhotoAsset],
    analyses: &HashMap<String, AnalysisRow>,
    models_id: &str,
) -> Vec<(PhotoAsset, Need)> {
    assets
        .iter()
        .filter_map(|a| {
            let need = match analyses.get(&a.id) {
                Some(r)
                    if r.file_size == a.primary.size
                        && r.file_mtime == a.primary.modified_ms
                        && r.metrics.is_some()
                        && r.models == models_id =>
                {
                    if r.thumb_path.as_ref().is_some_and(|p| p.exists()) {
                        return None;
                    }
                    Need::ThumbOnly
                }
                _ => Need::Full,
            };
            Some((a.clone(), need))
        })
        .collect()
}

/// Photos of `todo` analysed for the first time (no analysis row at all):
/// the only ones counted on the monthly quota.
fn first_analyses(todo: &[(PhotoAsset, Need)], analyses: &HashMap<String, AnalysisRow>) -> Vec<String> {
    todo.iter()
        .filter(|(a, need)| *need == Need::Full && !analyses.contains_key(&a.id))
        .map(|(a, _)| a.id.clone())
        .collect()
}

/// How many of `first_time` were not analysed after all (no measures).
fn not_analysed(first_time: &[String], analyses: &HashMap<String, AnalysisRow>) -> u32 {
    first_time.iter().filter(|id| analyses.get(*id).is_none_or(|r| r.metrics.is_none())).count() as u32
}

/// Quota of a run, given the photos `reserved` by an earlier run that never
/// ended: how many of `first_time` to reserve (those not reserved yet), and
/// how many reserved ones to give back now (no longer to analyse and not
/// analysed: gone from the folder, unreadable).
fn quota_plan(
    first_time: &[String],
    reserved: &HashSet<String>,
    analyses: &HashMap<String, AnalysisRow>,
) -> (u32, u32) {
    let to_count = first_time.iter().filter(|id| !reserved.contains(*id)).count() as u32;
    let wanted: HashSet<&String> = first_time.iter().collect();
    let stale = reserved
        .iter()
        .filter(|id| !wanted.contains(id) && analyses.get(*id).is_none_or(|r| r.metrics.is_none()))
        .count() as u32;
    (to_count, stale)
}

struct Decoded {
    preview: cull_decode::DecodedPreview,
    file: SourceFile,
}

/// Decode the primary file, falling back to a JPEG twin.
fn decode(asset: &PhotoAsset) -> std::result::Result<Decoded, cull_decode::DecodeError> {
    match cull_decode::load_preview(&asset.primary, ANALYSIS_EDGE) {
        Ok(p) => Ok(Decoded { preview: p, file: asset.primary.clone() }),
        Err(e) => {
            for c in asset.companions.iter().filter(|c| c.kind != FileKind::Raw) {
                if let Ok(p) = cull_decode::load_preview(c, ANALYSIS_EDGE) {
                    return Ok(Decoded { preview: p, file: c.clone() });
                }
            }
            Err(e)
        }
    }
}

fn thumb_path(engine: &Engine, project_id: &str, photo_id: &str) -> std::path::PathBuf {
    engine.project_cache_dir(project_id).join(format!("{photo_id}.t.jpg"))
}

fn write_thumb(engine: &Engine, project_id: &str, photo_id: &str, img: &image::RgbImage) -> Option<std::path::PathBuf> {
    let small = cull_decode::resize_long_edge(img, THUMB_EDGE);
    let path = thumb_path(engine, project_id, photo_id);
    match write_atomic(&path, &cull_decode::encode_jpeg(&small, 82)) {
        Ok(()) => Some(path),
        Err(e) => {
            log::warn!("thumbnail write failed: {e}");
            None
        }
    }
}

/// Row of a photo that could not be analysed this time: its previous
/// analysis when it had one (a drive unplugged or a file locked during a
/// re-analysis must not erase results), else an error row with `exif`.
fn failed_row(
    asset: &PhotoAsset,
    models: &str,
    err: String,
    state: &Mutex<ProjectState>,
    exif: impl FnOnce() -> Option<ExifInfo>,
) -> AnalysisRow {
    let previous = state.lock().analyses.get(&asset.id).filter(|r| r.metrics.is_some()).cloned();
    if let Some(mut row) = previous {
        log::warn!("{}: {err}; previous analysis kept", asset.rel_path);
        row.thumb_path = row.thumb_path.filter(|p| p.exists());
        return row;
    }
    AnalysisRow {
        photo_id: asset.id.clone(),
        file_size: asset.primary.size,
        file_mtime: asset.primary.modified_ms,
        models: models.to_string(),
        exif: exif(),
        metrics: None,
        faces: Vec::new(),
        aesthetic: None,
        embedding: None,
        search_embedding: None,
        subject: None,
        thumb_path: None,
        error: Some(err),
        duration_ms: 0,
        analyzed_at: now_ms(),
    }
}

/// Faces whose skin and person are measured (largest first).
const MEASURED_FACES: usize = 8;

/// Measures + ML on an upright image (Standard precision).
pub fn analyze_image(
    img: &RgbImage,
    models: Option<&cull_ml::ModelSet>,
) -> (TechnicalMetrics, MlObservations, Option<String>) {
    analyze_image_with_detail(img, models, None, &|_| None)
}

/// [`analyze_image`], in Maximum precision when `maximum` is given, with a
/// loader of a more detailed version of the same photo for small faces
/// (called at most once, with the long edge wanted; see
/// `cull_ml::ModelSet::analyze_with_detail`).
pub fn analyze_image_with_detail(
    img: &RgbImage,
    models: Option<&cull_ml::ModelSet>,
    maximum: Option<&cull_ml::MaximumModels>,
    detail: &dyn Fn(u32) -> Option<RgbImage>,
) -> (TechnicalMetrics, MlObservations, Option<String>) {
    let metrics = cull_analysis::compute_metrics(img);
    let cell: OnceCell<Option<RgbImage>> = OnceCell::new();
    let get = |long: u32| cell.get_or_init(|| detail(long)).as_ref();
    let (mut ml, warning) = match models {
        Some(m) => {
            match maximum.map_or_else(|| m.analyze_with_detail(img, &get), |x| m.analyze_maximum(x, img, &get)) {
                Ok(ml) => (ml, None),
                Err(e) => (MlObservations::default(), Some(format!("ml: {e}"))),
            }
        }
        None => (MlObservations::default(), None),
    };
    if let Some(subject) = ml.subject.as_mut() {
        let (focus, over) = cull_analysis::subject_measures(img, subject);
        subject.sharpness = focus;
        subject.ev_over = over;
    }
    // The detail image, when it was loaded and is the same picture.
    let ratio = |w: u32, h: u32| w as f32 / h.max(1) as f32;
    let fine = cell.get().and_then(Option::as_ref).filter(|d| {
        d.width() > img.width() && (ratio(d.width(), d.height()) / ratio(img.width(), img.height()) - 1.0).abs() < 0.01
    });
    // Fallback region focus (eyes not measurable) is calibrated on the
    // analysis size: large decodes (PNG/TIFF kept at full size) are measured
    // on a half-size copy.
    let focus_img;
    let focus_src = if img.width().max(img.height()) >= 2 * crate::ANALYSIS_EDGE {
        focus_img = cull_decode::resize_long_edge(img, img.width().max(img.height()) / 2);
        &focus_img
    } else {
        img
    };
    let to_1440 = 1440.0 / img.width().max(img.height()).max(1) as f32;
    for (k, face) in ml.faces.iter_mut().enumerate() {
        if face.sharpness.is_none() {
            // The eyes, at a fixed scale: faces too small in the preview
            // to reach it are read on the detail image.
            let iod = {
                let [r, l] = [face.landmarks[0], face.landmarks[1]];
                (((l.x - r.x) * img.width() as f32).powi(2) + ((l.y - r.y) * img.height() as f32).powi(2)).sqrt()
            };
            let src = match fine {
                Some(d) if iod * to_1440 < cull_analysis::EYE_FOCUS_IOD_PX => d,
                _ => img,
            };
            face.sharpness = cull_analysis::eye_sharpness(src, face)
                .or_else(|| Some(cull_analysis::region_sharpness(focus_src, face_focus_region(face))));
        }
        if k < MEASURED_FACES {
            if let Some((median, clipped)) = cull_analysis::skin_exposure(img, face) {
                face.skin_luma = Some(median);
                face.skin_clipped = Some(clipped);
            }
            face.body_hash = cull_analysis::body_hash(img, face);
        }
    }
    (metrics, ml, warning)
}

/// A more detailed decode of `asset` than its `preview_long` px analysis
/// preview, with a long edge of at least `long_edge` when the source allows:
/// the primary file's largest image (a RAW's full-size JPEG, a JPEG decoded
/// itself), else a JPEG twin (a Sony ARW only embeds a 1616 px preview).
/// `None` when no file holds more detail.
pub fn load_detail(asset: &PhotoAsset, preview_long: u32, long_edge: u32) -> Option<RgbImage> {
    let twins = asset.companions.iter().filter(|c| c.kind == FileKind::Jpeg);
    for file in std::iter::once(&asset.primary).chain(twins) {
        if let Ok(d) = cull_decode::load_preview(file, long_edge) {
            if d.image.width().max(d.image.height()) as f32 >= 1.25 * preview_long as f32 {
                return Some(d.image);
            }
        }
    }
    None
}

pub(crate) struct AnalysisJob<'a> {
    pub(crate) engine: &'a Arc<Engine>,
    pub(crate) project: ProjectRow,
    pub(crate) cancel: Arc<AtomicBool>,
    /// Count first-time photos on the Pelli monthly quota: the app, unless
    /// built without account. Never the CLI and tests.
    pub(crate) metered: bool,
}

impl AnalysisJob<'_> {
    fn emit_progress(&self, stage: Stage, done: u32, total: u32, current: Option<String>, started: Instant) {
        let elapsed = started.elapsed().as_secs_f32().max(0.001);
        let rate = done as f32 / elapsed;
        let eta = (rate > 0.0 && total > done).then(|| ((total - done) as f32 / rate * 1000.0) as u64);
        let p = Progress { stage, done, total, current, photos_per_second: rate, eta_ms: eta };
        let mut v = serde_json::to_value(p).unwrap_or_default();
        v["projectId"] = self.project.id.clone().into();
        self.engine.events().emit("analysis-progress", v);
    }

    pub(crate) fn run(self) -> Result<AnalysisSummary> {
        let _awake = crate::system::KeepAwake::new("Pelli Instant: analysing photos");
        let engine = self.engine;
        let pid = self.project.id.clone();
        let started = Instant::now();
        let state = engine.state(&pid)?;

        // Fresh scan so new files are picked up.
        self.emit_progress(Stage::Scanning, 0, 0, None, started);
        let opts = cull_decode::ScanOptions { recursive: self.project.recursive, ..Default::default() };
        let assets = cull_decode::scan_folder(&self.project.root_path, &opts)?;
        engine.store.lock().sync_photos(&pid, &assets)?;
        {
            let mut st = state.lock();
            st.assets = engine.store.lock().photos(&pid)?;
        }

        let models = engine.models();
        // Maximum precision when it is set and the pack is ready (its models
        // load now and are released after the analysis); otherwise Standard.
        let wants_maximum = engine.settings.read().precision == Precision::Maximum && models.is_some();
        if wants_maximum && engine.maximum_ready() {
            // Some seconds before the first photo: say so.
            self.emit_progress(Stage::Loading, 0, 0, None, started);
        }
        let maximum = if wants_maximum { engine.maximum_models() } else { None };
        let precision = if maximum.is_some() { Precision::Maximum } else { Precision::Standard };
        let models_id = match (&models, &maximum) {
            (Some(m), Some(x)) => m.models_id_maximum(x),
            (Some(m), None) => m.models_id(),
            (None, _) => METRICS_ONLY.to_string(),
        };
        let (todo, first_time) = {
            let st = state.lock();
            let todo = work_to_do(&st.assets, &st.analyses, &models_id);
            let first_time = first_analyses(&todo, &st.analyses);
            (todo, first_time)
        };
        log::info!(
            "analysis of {pid}: {} photo(s) to process ({} new), models {models_id}",
            todo.len(),
            first_time.len()
        );

        // Photos reserved by a run that never ended (app quit, crash) are
        // still counted: never reserved again.
        let reserved = if self.metered { engine.reserved_photos(&pid) } else { HashSet::new() };
        let (to_count, stale) = quota_plan(&first_time, &reserved, &state.lock().analyses);
        let give_back = match self.metered.then(|| engine.reserve_usage(to_count)) {
            Some(Err(e)) => {
                engine.release_maximum_models();
                return Err(e);
            }
            Some(Ok(limited)) => limited || !reserved.is_empty(),
            None => false,
        };
        if give_back {
            engine.set_reserved_photos(&pid, &first_time);
        }
        let result = self.analyse(&state, &todo, models.as_deref(), maximum.as_deref(), precision, &models_id, started);
        drop(maximum);
        engine.release_maximum_models();
        if give_back {
            // Cancelled, failed or unreadable photos are not counted.
            let unused = not_analysed(&first_time, &state.lock().analyses);
            engine.release_usage(unused + stale);
            engine.clear_reserved_photos(&pid);
        }
        result
    }

    /// Analyse `todo`, save as results come in, then decide and report.
    #[allow(clippy::too_many_arguments)]
    fn analyse(
        &self,
        state: &Mutex<ProjectState>,
        todo: &[(PhotoAsset, Need)],
        models: Option<&cull_ml::ModelSet>,
        maximum: Option<&cull_ml::MaximumModels>,
        precision: Precision,
        models_id: &str,
        started: Instant,
    ) -> Result<AnalysisSummary> {
        let engine = self.engine;
        let pid = self.project.id.as_str();
        let total = todo.len() as u32;
        let threads = match maximum {
            // More workers only wait for the pack's heavy models, holding
            // decoded photos in memory.
            Some(x) => engine.worker_threads().min(x.max_workers()),
            None => engine.worker_threads(),
        };
        let background = engine.settings.read().background;
        if let Some(m) = models {
            // Normally done when the models loaded: free then.
            m.prepare_for(threads, precision);
        }
        if let Some(x) = maximum {
            x.prepare(threads);
        }
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("cull-worker-{i}"))
            .start_handler(move |_| crate::system::set_worker_priority(background))
            .build()
            .map_err(|e| EngineError::Remote { code: "threads".into(), message: e.to_string() })?;

        let (tx, rx) = crossbeam_channel::bounded::<AnalysisRow>(512);
        std::thread::scope(|scope| -> Result<()> {
            // Workers.
            scope.spawn(move || {
                pool.install(|| {
                    use rayon::prelude::*;
                    todo.par_iter().for_each_with(tx, |tx, (asset, need)| {
                        if self.cancel.load(Ordering::Relaxed) {
                            return;
                        }
                        // A bug on one photo must not stop the run.
                        let row = std::panic::catch_unwind(AssertUnwindSafe(|| {
                            self.process(asset, *need, models, maximum, models_id, state)
                        }))
                        .unwrap_or_else(|_| {
                            log::error!("analysis of {} panicked", asset.rel_path);
                            failed_row(asset, models_id, "internal error".into(), state, || None)
                        });
                        let _ = tx.send(row);
                    });
                });
            });

            // Collector (this thread).
            let mut done = 0u32;
            let mut batch: Vec<AnalysisRow> = Vec::new();
            let mut pending_views: Vec<String> = Vec::new();
            let mut last_flush = Instant::now();
            let mut last_decide = Instant::now();
            let mut failure: Option<EngineError> = None;
            loop {
                let msg = rx.recv_timeout(Duration::from_millis(200));
                let disconnected = matches!(msg, Err(crossbeam_channel::RecvTimeoutError::Disconnected));
                if failure.is_some() {
                    // Results can no longer be saved: the workers stop at
                    // their next photo; keep receiving so none of them
                    // blocks on a full channel.
                    if disconnected {
                        break;
                    }
                    continue;
                }
                if let Ok(row) = msg {
                    done += 1;
                    pending_views.push(row.photo_id.clone());
                    batch.push(row);
                }
                let flush_due = batch.len() >= 64 || last_flush.elapsed() > Duration::from_millis(400);
                if (flush_due || disconnected) && !batch.is_empty() {
                    if let Err(e) = engine.store.lock().save_analyses(pid, &batch) {
                        log::error!("analysis results not saved: {e}");
                        self.cancel.store(true, Ordering::Relaxed);
                        failure = Some(e);
                        continue;
                    }
                    let mut st = state.lock();
                    for row in batch.drain(..) {
                        st.analyses.insert(row.photo_id.clone(), row);
                    }
                    if last_decide.elapsed() > Duration::from_millis(1000) || disconnected {
                        st.decide(&self.project.settings);
                        last_decide = Instant::now();
                    }
                    let ids: HashSet<String> = pending_views.drain(..).collect();
                    for a in st.assets.iter().filter(|a| ids.contains(&a.id)) {
                        let view = engine.photo_view(&st, a);
                        engine.events().emit("analysis-photo", serde_json::json!({ "projectId": pid, "photo": view }));
                    }
                    drop(st);
                    last_flush = Instant::now();
                    self.emit_progress(Stage::Analyzing, done, total, None, started);
                }
                if disconnected {
                    break;
                }
            }
            failure.map_or(Ok(()), Err)
        })?;

        let cancelled = self.cancel.load(Ordering::Relaxed);
        self.emit_progress(Stage::Grouping, total, total, None, started);
        let mut project = self.project.clone();
        let summary = {
            let mut st = state.lock();
            st.decide(&project.settings);
            let mut counts = DecisionCounts::default();
            let mut reasons: BTreeMap<String, u32> = BTreeMap::new();
            let mut analyzed = 0;
            let mut failed = 0;
            for a in &st.assets {
                match st.analyses.get(&a.id) {
                    Some(r) if r.metrics.is_some() => analyzed += 1,
                    Some(_) => failed += 1,
                    None => {}
                }
                let assessment = st.assessments.get(&a.id);
                if let Some(x) = assessment {
                    for r in &x.reasons {
                        *reasons.entry(reason_key(*r)).or_default() += 1;
                    }
                }
                counts.add(final_decision(
                    assessment.map(|x| x.verdict),
                    &st.marks.get(&a.id).copied().unwrap_or_default(),
                ));
            }
            AnalysisSummary {
                project_id: pid.to_string(),
                analyzed,
                failed,
                duration_ms: started.elapsed().as_millis() as u64,
                counts,
                reasons,
                groups: st.groups.len() as u32,
                precision,
            }
        };
        if !cancelled {
            project.analyzed_at = Some(now_ms());
        }
        if summary.analyzed > 0 {
            project.precision = Some(precision);
        }
        engine.store.lock().upsert_project(&project)?;
        self.emit_progress(Stage::Done, total, total, None, started);
        engine.events().emit("analysis-done", serde_json::to_value(&summary)?);
        Ok(summary)
    }

    /// Analyse one photo, or only regenerate its thumbnail.
    fn process(
        &self,
        asset: &PhotoAsset,
        need: Need,
        models: Option<&cull_ml::ModelSet>,
        maximum: Option<&cull_ml::MaximumModels>,
        models_id: &str,
        state: &Mutex<ProjectState>,
    ) -> AnalysisRow {
        let t0 = Instant::now();
        let decoded = match decode(asset) {
            Ok(d) => d,
            Err(e) => {
                return failed_row(asset, models_id, e.to_string(), state, || {
                    cull_decode::read_exif(&asset.primary.path).ok()
                })
            }
        };
        let thumb = write_thumb(self.engine, &self.project.id, &asset.id, &decoded.preview.image);
        if need == Need::ThumbOnly {
            if let Some(mut row) = state.lock().analyses.get(&asset.id).cloned() {
                row.thumb_path = thumb;
                return row;
            }
        }
        let mut exif = decoded.preview.exif.clone();
        if decoded.file.path != asset.primary.path {
            // Fallback twin decoded: keep the RAW's EXIF when readable.
            if let Ok(e) = cull_decode::read_exif(&asset.primary.path) {
                exif = e;
            }
        }
        if exif.width.is_none() {
            exif.width = Some(decoded.preview.source_width);
            exif.height = Some(decoded.preview.source_height);
        }
        let preview_long = decoded.preview.image.width().max(decoded.preview.image.height());
        let detail = |long: u32| load_detail(asset, preview_long, long);
        let (metrics, ml, warning) = analyze_image_with_detail(&decoded.preview.image, models, maximum, &detail);
        AnalysisRow {
            photo_id: asset.id.clone(),
            file_size: asset.primary.size,
            file_mtime: asset.primary.modified_ms,
            models: models_id.to_string(),
            exif: Some(exif),
            metrics: Some(metrics),
            embedding: ml.embedding,
            search_embedding: ml.search_embedding,
            subject: ml.subject,
            faces: ml.faces,
            aesthetic: ml.aesthetic,
            thumb_path: thumb,
            error: warning,
            duration_ms: t0.elapsed().as_millis() as u64,
            analyzed_at: now_ms(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn asset(id: &str, size: u64) -> PhotoAsset {
        PhotoAsset {
            id: id.into(),
            rel_path: format!("{id}.jpg"),
            stem: id.into(),
            primary: SourceFile {
                path: PathBuf::from(format!("{id}.jpg")),
                kind: FileKind::Jpeg,
                size,
                modified_ms: 1,
            },
            companions: vec![],
            xmp_sidecar: None,
        }
    }

    fn row(id: &str, size: u64, models: &str, measured: bool) -> AnalysisRow {
        AnalysisRow {
            photo_id: id.into(),
            file_size: size,
            file_mtime: 1,
            models: models.into(),
            exif: None,
            metrics: measured.then(TechnicalMetrics::default),
            faces: vec![],
            aesthetic: None,
            embedding: None,
            search_embedding: None,
            subject: None,
            thumb_path: None,
            error: (!measured).then(|| "unreadable".into()),
            duration_ms: 1,
            analyzed_at: 1,
        }
    }

    #[test]
    fn only_first_analyses_are_counted() {
        let assets =
            vec![asset("new", 10), asset("done", 10), asset("changed", 20), asset("failed", 10), asset("old", 10)];
        let analyses: HashMap<String, AnalysisRow> = [
            row("done", 10, "m2", true),    // up to date (thumbnail lost)
            row("changed", 10, "m2", true), // file changed since
            row("failed", 10, "m2", false), // previous error
            row("old", 10, "m1", true),     // older models
        ]
        .into_iter()
        .map(|r| (r.photo_id.clone(), r))
        .collect();
        let todo = work_to_do(&assets, &analyses, "m2");
        let needs: Vec<(&str, Need)> = todo.iter().map(|(a, n)| (a.id.as_str(), *n)).collect();
        assert_eq!(
            needs,
            [
                ("new", Need::Full),
                ("done", Need::ThumbOnly),
                ("changed", Need::Full),
                ("failed", Need::Full),
                ("old", Need::Full)
            ]
        );
        assert_eq!(first_analyses(&todo, &analyses), ["new"]);
    }

    #[test]
    fn photos_reserved_by_an_unfinished_run_are_not_counted_twice() {
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let analyses: HashMap<String, AnalysisRow> = [row("done", 10, "m", true), row("unreadable", 10, "m", false)]
            .into_iter()
            .map(|r| (r.photo_id.clone(), r))
            .collect();
        // The quit run reserved a, b, done, unreadable and gone (deleted since).
        let reserved: HashSet<String> = ids(&["a", "b", "done", "unreadable", "gone"]).into_iter().collect();
        let (to_count, stale) = quota_plan(&ids(&["a", "b", "new"]), &reserved, &analyses);
        assert_eq!(to_count, 1, "only `new` is reserved");
        assert_eq!(stale, 2, "`unreadable` and `gone` are given back, `done` stays counted");
        assert_eq!(quota_plan(&ids(&["x"]), &HashSet::new(), &analyses), (1, 0));
    }

    #[test]
    fn a_save_error_ends_the_run_instead_of_blocking_it() {
        let tmp = tempfile::tempdir().unwrap();
        let shoot = tmp.path().join("shoot");
        std::fs::create_dir_all(&shoot).unwrap();
        let jpeg = cull_decode::encode_jpeg(&RgbImage::from_pixel(32, 24, image::Rgb([90, 120, 150])), 80);
        // More photos than the results channel holds.
        for i in 0..700 {
            std::fs::write(shoot.join(format!("IMG_{i:04}.jpg")), &jpeg).unwrap();
        }
        let engine = Engine::open(crate::EngineConfig {
            data_dir: tmp.path().join("data"),
            cache_dir: tmp.path().join("cache"),
            models_dir: tmp.path().join("no-models"),
            pack_dir: None,
            url_mapper: Arc::new(|p: &Path| p.display().to_string()),
            events: Arc::new(crate::NullSink),
            secrets: None,
        })
        .unwrap();
        engine.set_worker_threads(Some(2));
        let project = engine.open_folder(&shoot, false).unwrap();
        // The disk fills up: no result can be saved.
        engine.store.lock().execute_for_tests(
            "CREATE TRIGGER full BEFORE INSERT ON analysis BEGIN SELECT RAISE(FAIL, 'database or disk is full'); END;",
        );
        let (tx, rx) = std::sync::mpsc::channel();
        let e = engine.clone();
        std::thread::spawn(move || tx.send(e.analyze_blocking(&project.id, cull_types::CullSettings::default())));
        let result = rx.recv_timeout(Duration::from_secs(60)).expect("the analysis ended");
        assert!(matches!(result, Err(EngineError::Db(_))), "{result:?}");
    }

    #[test]
    fn unanalysed_first_photos_are_given_back() {
        let first_time: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        let mut analyses: HashMap<String, AnalysisRow> = HashMap::new();
        assert_eq!(not_analysed(&first_time, &analyses), 3, "cancelled before any photo");
        analyses.insert("a".into(), row("a", 10, "m", true));
        analyses.insert("b".into(), row("b", 10, "m", false));
        assert_eq!(not_analysed(&first_time, &analyses), 2, "b unreadable, c not reached");
    }
}
