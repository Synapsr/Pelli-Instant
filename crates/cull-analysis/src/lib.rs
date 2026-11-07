//! Classic computer vision and decision logic of Pelli Instant.
//!
//! No neural network here: these measures are cheap (a few ms per photo),
//! deterministic and explainable, which is what lets the photographer trust
//! the verdicts ("rejected: blurry" must be visibly true).
//!
//! CONTRACT — these signatures are used by `cull-engine` and must stay
//! stable.
//!
//! Where things are explained:
//! * focus (edge width, tiles, noise, motion blur): `sharpness` module docs;
//! * exposure (stops off, low-key / high-key tolerance): `exposure`;
//! * perceptual hashes: `hash`;
//! * groups and scenes: `group`;
//! * scores, defects, verdicts and every threshold: `score`;
//! * XMP sidecars: `xmp`;
//! * plain-language version for photographers: `README.md` of this crate.

mod exposure;
mod group;
mod hash;
mod prep;
mod score;
mod sharpness;
mod xmp;

use cull_types::{
    Assessment, CullSettings, ExifInfo, ExposureMetrics, FaceObservation, ImageHashes, NormRect, Scene,
    SharpnessMetrics, SimilarGroup, SubjectObservation, TechnicalMetrics, UserMark,
};
use image::RgbImage;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("i/o error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid xmp in {path}: {reason}")]
    InvalidXmp { path: String, reason: String },
}

pub type Result<T> = std::result::Result<T, AnalysisError>;

// ---------------------------------------------------------------------------
// Per-photo measures
// ---------------------------------------------------------------------------

/// Long edge (px) of the grayscale working copy used by [`compute_metrics`].
pub const WORKING_LONG_EDGE: u32 = prep::WORK_LONG_EDGE as u32;

/// Regions smaller than this (px, either side) are enlarged around their
/// centre to this size by [`region_sharpness`], so tiny faces are measured on
/// enough pixels (their immediate surroundings share their focus distance).
pub const MIN_REGION_PX: u32 = 32;

/// Regions larger than this (px, long edge) are area-downscaled to it by
/// [`region_sharpness`]: a 1 500 px close-up is judged at a normal viewing
/// scale, and the cost stays bounded (≈1 ms).
pub const MAX_REGION_PX: u32 = 640;

/// Focus score returned when there is nothing to measure (no edge in the
/// region, degenerate input): neither sharp nor blurry.
pub const UNKNOWN_FOCUS: f32 = sharpness::UNKNOWN_FOCUS;

/// Sharpness, exposure and perceptual hashes of one photo.
///
/// `img` is the upright preview from cull-decode (long edge ~1440-3000 px);
/// the function downsizes internally to a fixed working size (1024 px long
/// edge, grayscale) so values are comparable between photos whatever the
/// camera. Target: < 15 ms on one laptop core.
///
/// Images smaller than the working size are analysed at their own size
/// (never upscaled). An empty image gives all-zero metrics.
pub fn compute_metrics(img: &RgbImage) -> TechnicalMetrics {
    let g = prep::luma_fit(img, prep::WORK_LONG_EDGE);
    if g.is_empty() {
        return TechnicalMetrics::default();
    }
    let f = sharpness::frame_focus(&g);
    let e = exposure::exposure(&g);
    TechnicalMetrics {
        sharpness: SharpnessMetrics {
            global: f.global_energy,
            peak: f.peak_energy,
            subject: None,
            motion_blur: f.motion,
            score: f.score,
        },
        exposure: ExposureMetrics {
            mean_luma: e.mean,
            median_luma: e.median,
            highlights_clipped: e.clip_hi,
            shadows_clipped: e.clip_lo,
            contrast: e.contrast,
            score: e.score,
        },
        hashes: ImageHashes { phash: hash::phash(&g), dhash: hash::dhash(&g) },
    }
}

/// Focus score in `[0, 1]` of a region (face, eyes), measured on `img` at its
/// native resolution so small faces keep their detail. Normalised on the same
/// scale as `SharpnessMetrics::score`.
///
/// The score is the sharpest 20% of the region's edges (and never sharper
/// than its blurrier direction, so a face smeared by motion does not pass)
/// mapped through the same width → score curve as the frame. A small region
/// with almost no edge is measured again with the noise of a context three
/// times larger around it (its own flattest quarter was face texture). Regions smaller than
/// [`MIN_REGION_PX`] are enlarged around their centre, regions larger than
/// [`MAX_REGION_PX`] are downscaled. Because a crop is measured at the
/// preview's native resolution (finer than the 1024 px working copy), its
/// values run a little lower than frame scores for the same optical
/// sharpness; `assess` judges faces against the shoot's own face scores.
/// A region with no measurable edge, outside the image, or with non-finite
/// coordinates returns [`UNKNOWN_FOCUS`].
pub fn region_sharpness(img: &RgbImage, region: NormRect) -> f32 {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let r = region;
    if w < 1.0 || h < 1.0 || ![r.x, r.y, r.w, r.h].iter().all(|v| v.is_finite()) {
        return UNKNOWN_FOCUS;
    }
    let (ax, bx) = (r.x.min(r.x + r.w), r.x.max(r.x + r.w));
    let (ay, by) = (r.y.min(r.y + r.h), r.y.max(r.y + r.h));
    let (cx, cy) = ((ax + bx) * 0.5, (ay + by) * 0.5);
    if !(0.0..=1.0).contains(&cx) || !(0.0..=1.0).contains(&cy) {
        return UNKNOWN_FOCUS;
    }
    let span = |a: f32, b: f32, size: f32, min_px: f32| -> (usize, usize) {
        let min = min_px.min(size);
        let (mut lo, mut hi) = ((a * size).floor().max(0.0), (b * size).ceil().min(size));
        if hi - lo < min {
            let c = ((a + b) * 0.5 * size).clamp(0.0, size);
            lo = (c - min * 0.5).round().clamp(0.0, size - min);
            hi = lo + min;
        }
        (lo as usize, hi as usize)
    };
    let (x0, x1) = span(ax, bx, w, MIN_REGION_PX as f32);
    let (y0, y1) = span(ay, by, h, MIN_REGION_PX as f32);
    let g = prep::luma_rect(img, x0, y0, x1, y1, MAX_REGION_PX as usize);
    let (score, _, edges) = sharpness::region_focus(&g, None);
    if edges >= REGION_RETRY_EDGES || (x1 - x0).max(y1 - y0) >= REGION_CONTEXT_BELOW_PX {
        return score;
    }
    // A small crop with (almost) no edge: its flattest quarter was face
    // texture, not noise, so the noise threshold hid its edges. Measure the
    // noise on a context 3× larger (≥ 128 px) at the same native scale.
    let side = ((x1 - x0).max(y1 - y0) as f32 * 3.0).max(REGION_CONTEXT_BELOW_PX as f32);
    let (cx, cy) = ((x0 + x1) as f32 * 0.5 / w, (y0 + y1) as f32 * 0.5 / h);
    let (hx, hy) = (side * 0.5 / w, side * 0.5 / h);
    let (cx0, cx1) = span(cx - hx, cx + hx, w, side);
    let (cy0, cy1) = span(cy - hy, cy + hy, h, side);
    match sharpness::estimate_noise(&prep::luma_rect(img, cx0, cy0, cx1, cy1, usize::MAX)) {
        Some(noise) => sharpness::region_focus(&g, Some(noise)).0,
        None => score,
    }
}

/// Small regions (long side below `REGION_CONTEXT_BELOW_PX`) yielding fewer
/// than `REGION_RETRY_EDGES` edges are measured again with the noise of a
/// larger context.
const REGION_RETRY_EDGES: u32 = 25;
/// Regions whose long side is below this (px) get their noise measured on a
/// larger context.
const REGION_CONTEXT_BELOW_PX: usize = 128;

// ---------------------------------------------------------------------------
// Faces: focus of the eyes, skin exposure, pose of the person
// ---------------------------------------------------------------------------

/// Inter-ocular distance (px) at which [`eye_sharpness`] judges the eyes.
pub const EYE_FOCUS_IOD_PX: f32 = 64.0;
/// Each eye's box (centred on the eye) in inter-ocular distances: lids,
/// lashes, iris, not the brow nor the cheek. The measured band is the
/// bounding box of both (it follows a tilted head).
const EYE_PATCH_W: f32 = 1.0;
const EYE_PATCH_H: f32 = 0.55;
/// Noise of the eye band is measured on this context around both eyes
/// (inter-ocular distances); it is also the fallback region.
const EYE_CONTEXT_W: f32 = 2.4;
const EYE_CONTEXT_H: f32 = 1.6;
/// A band with fewer edges than this at the measuring scale is not judged
/// alone (eyes closed, hidden by hair or a hand): the area around them is.
const EYE_MIN_EDGES: u32 = 12;
/// Below this inter-ocular distance (px of the given image) eyes are not
/// measured.
const EYE_MIN_IOD_PX: f32 = 8.0;

/// Pixel rectangle `(x0, y0, x1, y1)` of a centred box, clipped to the image.
fn px_box(img: &RgbImage, cx: f32, cy: f32, w: f32, h: f32) -> Option<(usize, usize, usize, usize)> {
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    if ![cx, cy, w, h].iter().all(|v| v.is_finite()) || w <= 0.0 || h <= 0.0 {
        return None;
    }
    let x0 = (cx - w / 2.0).floor().clamp(0.0, iw);
    let x1 = (cx + w / 2.0).ceil().clamp(0.0, iw);
    let y0 = (cy - h / 2.0).floor().clamp(0.0, ih);
    let y1 = (cy + h / 2.0).ceil().clamp(0.0, ih);
    (x1 - x0 >= 2.0 && y1 - y0 >= 2.0).then_some((x0 as usize, y0 as usize, x1 as usize, y1 as usize))
}

/// Eye centres (px) and inter-ocular distance of a face in `img`.
fn eyes_px(img: &RgbImage, face: &FaceObservation) -> Option<([f32; 2], [f32; 2], f32)> {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let [r, l] = [face.landmarks[0], face.landmarks[1]];
    let (r, l) = ([r.x * w, r.y * h], [l.x * w, l.y * h]);
    let iod = ((l[0] - r[0]).powi(2) + (l[1] - r[1]).powi(2)).sqrt();
    (iod.is_finite() && iod >= EYE_MIN_IOD_PX).then_some((r, l, iod))
}

/// The eye band and its context, as grayscale at the measuring scale.
fn eye_regions(img: &RgbImage, face: &FaceObservation) -> Option<(Option<prep::Gray>, prep::Gray)> {
    let (r, l, iod) = eyes_px(img, face)?;
    let scale = (EYE_FOCUS_IOD_PX / iod).min(1.0);
    let at = |(x0, y0, x1, y1): (usize, usize, usize, usize)| {
        let long = ((x1 - x0).max(y1 - y0) as f32 * scale).round().max(8.0) as usize;
        prep::luma_rect(img, x0, y0, x1, y1, long)
    };
    let (mx, my) = ((r[0] + l[0]) / 2.0, (r[1] + l[1]) / 2.0);
    let context = at(px_box(img, mx, my, EYE_CONTEXT_W * iod, EYE_CONTEXT_H * iod)?);
    let (bw, bh) = ((l[0] - r[0]).abs() + EYE_PATCH_W * iod, (l[1] - r[1]).abs() + EYE_PATCH_H * iod);
    let band = px_box(img, mx, my, bw, bh).map(at);
    Some((band, context))
}

/// Focus score in `[0, 1]` of a face's **eyes**, on the same scale as
/// [`region_sharpness`], or `None` when they cannot be placed (face too
/// small in `img`, no edge at all around them).
///
/// The band holding both eyes (lids, lashes, iris) is resampled so the
/// inter-ocular distance is [`EYE_FOCUS_IOD_PX`] (never enlarged) and judged
/// on its sharpest fifth of edges, so the **sharper eye** decides (at f/1.4
/// the far eye of a three-quarter face is soft), with the noise of the area
/// around both eyes. Cheeks, hair, a tilted head or a smile no longer pull
/// the measure down, and the same face reads the same on a 1620 px preview
/// or a 7360 px full-size JPEG (the face focus measured on the whole central
/// face changed from 0.3 to 0.9 with the face's size in pixels). When the
/// band shows too few edges (eyes closed or hidden), the area around both
/// eyes (brows, nose bridge) is measured at the same scale instead. Pass the
/// most detailed image available for small faces.
pub fn eye_sharpness(img: &RgbImage, face: &FaceObservation) -> Option<f32> {
    let (band, context) = eye_regions(img, face)?;
    let noise = sharpness::estimate_noise(&context);
    if let Some(band) = band {
        let (score, _, edges) = sharpness::region_focus(&band, noise);
        if edges >= EYE_MIN_EDGES {
            return Some(score);
        }
    }
    let (score, _, edges) = sharpness::region_focus(&context, noise);
    (edges > 0).then_some(score)
}

/// Core of the face box (forehead to chin, cheek to cheek): mostly skin,
/// whatever the glasses, beard or expression.
fn skin_box(img: &RgbImage, face: &FaceObservation) -> Option<(usize, usize, usize, usize)> {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let b = face.bbox;
    px_box(img, (b.x + b.w * 0.5) * w, (b.y + b.h * 0.55) * h, b.w * 0.6 * w, b.h * 0.6 * h)
}

/// Percentile of the face core read as its skin tone: above the median, so
/// sunglasses, a beard, nostrils or a shadowed eye socket (a third of the
/// core at most) do not darken it.
const SKIN_PERCENTILE: f32 = 0.65;

/// Exposure of the face skin: Rec. 709 luma in `[0, 1]` of the face core
/// (its 65th percentile) and share of blown pixels (≥ 250/255). `None` for a
/// degenerate box. The culling judges a portrait's exposure on it rather
/// than on the whole frame (a backlit portrait on a white sky, a lit face in
/// a dark workshop).
pub fn skin_exposure(img: &RgbImage, face: &FaceObservation) -> Option<(f32, f32)> {
    let (x0, y0, x1, y1) = skin_box(img, face)?;
    let g = prep::luma_rect(img, x0, y0, x1, y1, 96);
    if g.px.len() < 16 {
        return None;
    }
    let mut v = g.px.clone();
    v.sort_by(|a, b| a.total_cmp(b));
    let median = v[((v.len() - 1) as f32 * SKIN_PERCENTILE).round() as usize] / 255.0;
    let clipped = v.iter().filter(|&&p| p >= 250.0).count() as f32 / v.len() as f32;
    Some((median.clamp(0.0, 1.0), clipped))
}

/// Size of the person around a face, in face-box units: from `BODY_LEFT`
/// to `BODY_RIGHT` box widths around its centre, `BODY_UP` box heights
/// above its top to `BODY_DOWN` below its bottom (head, shoulders, arms,
/// hands at the waist).
const BODY_HALF_W: f32 = 2.0;
const BODY_UP: f32 = 0.4;
const BODY_DOWN: f32 = 3.5;
/// Canonical size of the person crop before hashing.
const BODY_W: usize = 27;
const BODY_H: usize = 36;

/// 64-bit gradient hash of the person around `face` (see
/// `FaceObservation::body_hash`). The crop is anchored on the face, so it
/// follows the subject when the framing moves; the part outside the photo
/// is filled with the crop's mean. `None` for a degenerate face box.
pub fn body_hash(img: &RgbImage, face: &FaceObservation) -> Option<u64> {
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    let b = face.bbox;
    let (fw, fh) = (b.w * iw, b.h * ih);
    if !(fw.is_finite() && fh.is_finite()) || fw < 4.0 || fh < 4.0 {
        return None;
    }
    let cx = (b.x + b.w / 2.0) * iw;
    let (rx0, rx1) = (cx - BODY_HALF_W * fw, cx + BODY_HALF_W * fw);
    let (ry0, ry1) = (b.y * ih - BODY_UP * fh, (b.y + b.h) * ih + BODY_DOWN * fh);
    let (cx0, cx1) = (rx0.max(0.0).floor(), rx1.min(iw).ceil());
    let (cy0, cy1) = (ry0.max(0.0).floor(), ry1.min(ih).ceil());
    if cx1 - cx0 < 2.0 || cy1 - cy0 < 2.0 {
        return None;
    }
    // Inside part at the canonical scale, then placed on the canvas.
    let (sx, sy) = (BODY_W as f32 / (rx1 - rx0), BODY_H as f32 / (ry1 - ry0));
    let dw = (((cx1 - cx0) * sx).round() as usize).clamp(1, BODY_W);
    let dh = (((cy1 - cy0) * sy).round() as usize).clamp(1, BODY_H);
    let (x0, y0, x1, y1) = (cx0 as usize, cy0 as usize, cx1 as usize, cy1 as usize);
    let inner = prep::luma_rect(img, x0, y0, x1, y1, 4 * dw.max(dh));
    let inner = prep::resize_exact(&inner, dw, dh);
    if inner.is_empty() {
        return None;
    }
    let mean = inner.px.iter().sum::<f32>() / inner.px.len() as f32;
    let ox = (((cx0 - rx0) * sx).round() as usize).min(BODY_W - dw);
    let oy = (((cy0 - ry0) * sy).round() as usize).min(BODY_H - dh);
    let mut px = vec![mean; BODY_W * BODY_H];
    for y in 0..dh {
        px[(oy + y) * BODY_W + ox..(oy + y) * BODY_W + ox + dw].copy_from_slice(&inner.px[y * dw..(y + 1) * dw]);
    }
    Some(hash::dhash(&prep::Gray { w: BODY_W, h: BODY_H, px }))
}

// ---------------------------------------------------------------------------
// Subject region (a person without a usable face, an animal)
// ---------------------------------------------------------------------------

/// Long side of the grid on which the subject mask's pixels are read.
const SUBJECT_GRID: usize = 256;

/// Focus and overexposure of a subject region found by a segmentation model
/// (`SubjectObservation`): the focus of its bounding box
/// ([`region_sharpness`]: its sharpest edges), and the stops of
/// overexposure of the pixels of its mask, read like the frame's (median
/// beyond half a stop over mid-grey, plus the blown share beyond 12%). A
/// figure seen from the back against a blown sky is then judged on itself,
/// not on the sky. `None` for a mask with too few pixels.
pub fn subject_measures(img: &RgbImage, subject: &SubjectObservation) -> (Option<f32>, Option<f32>) {
    let focus = Some(region_sharpness(img, subject.bbox)).filter(|&f| f != UNKNOWN_FOCUS);
    let Some(mask) = subject.mask.as_ref() else { return (focus, None) };
    let g = prep::luma_rect(img, 0, 0, img.width() as usize, img.height() as usize, SUBJECT_GRID);
    if g.is_empty() {
        return (focus, None);
    }
    let px: Vec<f32> = (0..g.h)
        .flat_map(|y| (0..g.w).map(move |x| (x, y)))
        .filter(|&(x, y)| mask.contains((x as f32 + 0.5) / g.w as f32, (y as f32 + 0.5) / g.h as f32))
        .map(|(x, y)| g.px[y * g.w + x])
        .collect();
    if px.len() < 64 {
        return (focus, None);
    }
    let stats = exposure::exposure(&prep::Gray { w: px.len(), h: 1, px });
    (focus, Some(stats.ev_over))
}

// ---------------------------------------------------------------------------
// Shoot-level decisions
// ---------------------------------------------------------------------------

/// Model that computed a photo's grouping embedding: their cosines live on
/// different scales (see the `group` module).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmbeddingSpace {
    /// CLIP ViT-B/32, letterboxed (Standard precision).
    #[default]
    ClipB32,
    /// DINOv2 ViT-B/14 CLS, squashed (Maximum precision).
    DinoV2,
}

/// Everything the decision step knows about one photo.
#[derive(Debug, Clone)]
pub struct PhotoFacts<'a> {
    pub id: &'a str,
    pub exif: &'a ExifInfo,
    pub metrics: &'a TechnicalMetrics,
    /// Faces with `sharpness` filled.
    pub faces: &'a [FaceObservation],
    /// L2-normalised grouping embedding, if the ML step ran.
    pub embedding: Option<&'a [f32]>,
    /// Which model computed `embedding`.
    pub embedding_space: EmbeddingSpace,
    pub aesthetic: Option<f32>,
    /// Subject region of a photo without a usable face, with its measures
    /// ([`subject_measures`]) filled.
    pub subject: Option<&'a SubjectObservation>,
}

/// Group similar shots: bursts and re-takes of the same moment.
///
/// Photos are ordered by capture time (then id); two photos join a group when
/// they are within `settings.group_window_s` of each other (same camera
/// serial when both are known) AND visually similar (embedding cosine >=
/// `similarity_threshold`, or pHash Hamming distance <= 10 when there is no
/// embedding). Identical hashes form `GroupKind::Exact` groups whatever the
/// time. Single-linkage inside the time window, but a group never spans more
/// than 5 minutes. Groups of size 1 are not returned. Ids start at 1.
/// `photo_ids` are ordered best first (by `assess` ranking rules: score).
///
/// Photos without capture time are compared, in the order given (natural
/// file-name order), with their next 8 photos using the same tests plus a
/// layout check (pHash distance ≤ 16); such a group spans at most 24
/// positions.
///
/// Details (camera make/model separation when serials are missing, copies of
/// one image joining a burst, `Burst` vs `Similar`): see the `group` module
/// docs. With `settings.group_similar` off, only `Exact` groups are returned;
/// with `settings.detect_exact_duplicates` off, no `Exact` group is.
pub fn group_similar(photos: &[PhotoFacts<'_>], settings: &CullSettings) -> Vec<SimilarGroup> {
    group::group_similar(photos, settings)
}

/// Split the shoot into scenes on capture-time gaps > `settings.scene_gap_s`.
/// Photos without capture time go to a last scene. Ids start at 1.
pub fn split_scenes(photos: &[PhotoFacts<'_>], settings: &CullSettings) -> Vec<Scene> {
    group::split_scenes(photos, settings)
}

/// Score every photo and decide Keep / Maybe / Reject.
///
/// Thresholds adapt to the shoot (e.g. focus is judged relative to the
/// project's own distribution as well as absolutely), and to
/// `settings.sensitivity`. Inside each group, the `keep_per_group` best
/// photos without blocking defect are `Keep`, the others `Reject` with
/// `Reason::Duplicate` (or `Maybe` if very close in score to the best).
/// Returns one `Assessment` per input, same order.
///
/// Every rule and constant is documented in the `score` module. Pure and
/// fast (≈1 ms for 5 000 photos): safe to re-run on every slider move.
/// With `settings.group_similar` off, `Burst`/`Similar` groups are ignored;
/// with `settings.detect_exact_duplicates` off, copies are not rejected as
/// such. Each `detect_*` switch and `artistic_blur` are documented there too.
pub fn assess(photos: &[PhotoFacts<'_>], groups: &[SimilarGroup], settings: &CullSettings) -> Vec<Assessment> {
    score::assess(photos, groups, settings)
}

// ---------------------------------------------------------------------------
// Writing results for other tools
// ---------------------------------------------------------------------------

/// Who chose a rating or label written to a sidecar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmpSource {
    /// The photographer, in Pelli Instant: always written.
    Photographer,
    /// Mapped from Pelli's decision: never replaces a value set in another
    /// tool (see the `xmp` module docs).
    Decision,
}

/// What the XMP sidecar should carry.
#[derive(Debug, Clone)]
pub struct XmpPayload {
    pub mark: UserMark,
    /// Who chose `mark.rating` (with the reject flag, the `-1` rating).
    pub rating_source: XmpSource,
    /// Who chose `mark.label`.
    pub label_source: XmpSource,
    /// Pelli's own namespace (`pelli:`): score 0..100, verdict, reasons.
    pub assessment: Option<Assessment>,
}

/// Create or update `<photo>.xmp` next to a RAW (Lightroom Classic, Capture
/// One, Bridge, darktable conventions: `IMG_1.CR3` → `IMG_1.xmp`; if an
/// existing sidecar is named `IMG_1.CR3.xmp`, update that one).
///
/// Writes `xmp:Rating` (-1 for a rejected flag when `mark.flag == Reject`
/// and no stars), `xmp:Label`, and `pelli:*`; preserves every other existing
/// element and attribute (develop settings, keywords…), and the rating and
/// label set in another tool unless the photographer chose them in Pelli.
/// Atomic: write a temp file then rename. Returns the sidecar path.
///
/// See the `xmp` module docs for the exact rules (who owns the rating and
/// label, `pelli:Flag`, attribute vs element form, unreadable and read-only
/// sidecars).
pub fn write_xmp_sidecar(photo_path: &Path, payload: &XmpPayload) -> Result<std::path::PathBuf> {
    xmp::write_sidecar(photo_path, payload)
}

// ---------------------------------------------------------------------------
// Calibration hooks (not part of the contract)
// ---------------------------------------------------------------------------

/// Internal measures exposed for `examples/calibrate.rs` and the tests.
/// Unstable: not part of the contract.
#[doc(hidden)]
pub mod internals {
    use super::*;

    #[derive(Debug, Clone, Copy, Default)]
    pub struct FocusDebug {
        pub work_w: usize,
        pub work_h: usize,
        /// Edge width of the sharpest region (px at working size).
        pub peak_width: Option<f32>,
        /// Median edge width of the frame.
        pub global_width: Option<f32>,
        pub edges: u32,
        pub confidence: f32,
        pub score: f32,
        pub motion_ratio: f32,
        pub motion: f32,
        pub noise_sigma: f32,
        /// Noise σ of a Sobel component of the re-blurred image.
        pub noise_blur_grad: f32,
        /// Classic measures (p90 of tiles), for benchmarking.
        pub vol_global: f32,
        pub vol_peak: f32,
        pub tenengrad_peak: f32,
        pub modlap_peak: f32,
    }

    fn p90(mut v: Vec<f32>) -> f32 {
        if v.is_empty() {
            return 0.0;
        }
        v.sort_by(|a, b| a.total_cmp(b));
        v[((v.len() - 1) as f32 * 0.9).round() as usize]
    }

    pub fn focus_debug(img: &RgbImage) -> FocusDebug {
        let g = prep::luma_fit(img, prep::WORK_LONG_EDGE);
        let (nx, ny) = sharpness::grid_for(g.w, g.h);
        let Some(a) = sharpness::analyze(&g, nx, ny, true) else {
            return FocusDebug { work_w: g.w, work_h: g.h, ..Default::default() };
        };
        let f = sharpness::frame_focus_from(&a);
        let nt = a.nx * a.ny;
        let noise_blur_grad = a.noise_blur_grad;
        let noise = 2.0 * a.noise_grad * a.noise_grad;
        let ten: Vec<f32> = (0..nt)
            .map(|t| {
                let px = a.tile_px[t].max(1) as f64;
                ((a.tile_ten[t] / px) as f32 - noise).max(0.0)
            })
            .collect();
        let ml: Vec<f32> = (0..nt).map(|t| (a.tile_modlap[t] / a.tile_px[t].max(1) as f64) as f32).collect();
        FocusDebug {
            work_w: g.w,
            work_h: g.h,
            peak_width: f.peak_width,
            global_width: f.global_width,
            edges: f.edges,
            confidence: f.confidence,
            score: f.score,
            motion_ratio: f.motion_ratio,
            motion: f.motion,
            noise_sigma: f.noise_sigma,
            noise_blur_grad,
            vol_global: f.global_energy,
            vol_peak: f.peak_energy,
            tenengrad_peak: p90(ten),
            modlap_peak: p90(ml),
        }
    }

    #[derive(Debug, Clone, Copy, Default)]
    pub struct ExposureDebug {
        pub median: f32,
        pub p02: f32,
        pub p98: f32,
        pub p999: f32,
        pub clip_hi: f32,
        pub clip_lo: f32,
        pub ev_under: f32,
        pub ev_over: f32,
        pub score: f32,
    }

    pub fn exposure_debug(img: &RgbImage) -> ExposureDebug {
        let g = prep::luma_fit(img, prep::WORK_LONG_EDGE);
        let e = exposure::exposure(&g);
        ExposureDebug {
            median: e.median,
            p02: e.p02,
            p98: e.p98,
            p999: e.p999,
            clip_hi: e.clip_hi,
            clip_lo: e.clip_lo,
            ev_under: e.ev_under,
            ev_over: e.ev_over,
            score: e.score,
        }
    }

    /// Time (ms) of each stage of `compute_metrics` on `img`.
    pub fn stage_timings(img: &RgbImage) -> Vec<(&'static str, f64)> {
        use std::time::Instant;
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
        let mut out = Vec::new();
        let t = Instant::now();
        let g = prep::luma_fit(img, prep::WORK_LONG_EDGE);
        out.push(("luma + resize", ms(t)));
        let t = Instant::now();
        let (nx, ny) = sharpness::grid_for(g.w, g.h);
        let a = sharpness::analyze(&g, nx, ny, false);
        out.push(("edge analysis", ms(t)));
        let t = Instant::now();
        if let Some(a) = &a {
            std::hint::black_box(sharpness::frame_focus_from(a));
        }
        out.push(("focus aggregation", ms(t)));
        let t = Instant::now();
        std::hint::black_box(exposure::exposure(&g));
        out.push(("exposure", ms(t)));
        let t = Instant::now();
        std::hint::black_box((hash::phash(&g), hash::dhash(&g)));
        out.push(("hashes", ms(t)));
        out
    }

    /// Region focus details: (score, sharpest-30% width, edges).
    pub fn region_debug(img: &RgbImage, region: NormRect) -> (f32, Option<f32>, u32) {
        let (w, h) = (img.width() as f32, img.height() as f32);
        let x0 = ((region.x * w).max(0.0)) as usize;
        let y0 = ((region.y * h).max(0.0)) as usize;
        let x1 = (((region.x + region.w) * w).min(w)) as usize;
        let y1 = (((region.y + region.h) * h).min(h)) as usize;
        let g = prep::luma_rect(img, x0, y0, x1, y1, MAX_REGION_PX as usize);
        sharpness::region_focus(&g, None)
    }

    /// Eye focus details of a face: (score, width, edges) of the eye band
    /// then of the area around both eyes, at the measuring scale.
    pub fn eye_debug(img: &RgbImage, face: &FaceObservation) -> Vec<(f32, Option<f32>, u32)> {
        let Some((band, context)) = eye_regions(img, face) else { return Vec::new() };
        let noise = sharpness::estimate_noise(&context);
        band.iter().chain(std::iter::once(&context)).map(|g| sharpness::region_focus(g, noise)).collect()
    }

    /// Width → score curve shared by frames and regions.
    pub fn width_to_score(width: f32) -> f32 {
        sharpness::width_to_score(width)
    }

    pub fn hamming(a: u64, b: u64) -> u32 {
        hash::hamming(a, b)
    }

    /// Direct properties of the top-level `rdf:Description`s of an XMP
    /// packet: `(namespace URI, local name, value)`.
    pub fn xmp_properties(xmp: &[u8]) -> std::result::Result<Vec<(String, String, String)>, String> {
        xmp::read_props(xmp)
    }

    /// Sidecar path `write_xmp_sidecar` would use for `photo`.
    pub fn xmp_sidecar_path(photo: &Path) -> Option<std::path::PathBuf> {
        xmp::sidecar_path(photo)
    }
}
