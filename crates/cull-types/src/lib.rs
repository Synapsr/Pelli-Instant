//! Shared data types of Pelli Instant.
//!
//! Every crate of the workspace (decode, analysis, ml, engine, desktop shell)
//! speaks these types. They are serialised in camelCase so the desktop UI
//! (TypeScript, `src/lib/types.ts`) reads them as-is. Keep this crate free of
//! heavy dependencies: serde only.
//!
//! Conventions:
//! - Normalised coordinates are in `[0, 1]`, origin top-left, relative to the
//!   *upright* image (EXIF orientation already applied).
//! - Timestamps are Unix milliseconds (`i64`). Capture times taken from EXIF
//!   carry no time zone: they are read as UTC so ordering and deltas stay
//!   correct within a shoot.
//! - Scores are `f32` in `[0, 1]` unless the field says otherwise.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

/// Kind of a source file, decided from its extension (and confirmed by its
/// magic bytes when decoding).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Jpeg,
    Png,
    Tiff,
    Heif,
    /// Camera RAW (CR2, CR3, NEF, ARW, RAF, DNG, ORF, RW2, PEF, SRW…).
    Raw,
}

impl FileKind {
    /// Lower-case extensions (without dot) recognised for each kind.
    pub const RAW_EXTENSIONS: &'static [&'static str] = &[
        "cr2", "cr3", "crw", "nef", "nrw", "arw", "srf", "sr2", "raf", "dng", "orf", "rw2", "pef", "srw", "3fr", "fff",
        "iiq", "erf", "mef", "mos", "mrw", "x3f", "kdc", "dcr", "rwl",
    ];

    pub fn from_extension(ext: &str) -> Option<FileKind> {
        let ext = ext.to_ascii_lowercase();
        match ext.as_str() {
            "jpg" | "jpeg" | "jpe" => Some(FileKind::Jpeg),
            "png" => Some(FileKind::Png),
            "tif" | "tiff" => Some(FileKind::Tiff),
            "heic" | "heif" | "hif" => Some(FileKind::Heif),
            e if Self::RAW_EXTENSIONS.contains(&e) => Some(FileKind::Raw),
            _ => None,
        }
    }
}

/// One file on disk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFile {
    pub path: PathBuf,
    pub kind: FileKind,
    /// Size in bytes.
    pub size: u64,
    /// Last modification, Unix milliseconds.
    pub modified_ms: i64,
}

/// One photo as the photographer sees it: a RAW and its JPEG twin
/// (`IMG_0001.CR3` + `IMG_0001.JPG`) form a single asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhotoAsset {
    /// Stable identifier inside a project: hex SHA-1 of the primary file path
    /// relative to the project root (lower-cased on case-insensitive systems).
    pub id: String,
    /// Path of `primary` relative to the project root, with `/` separators.
    pub rel_path: String,
    /// File name without extension (`IMG_0001`).
    pub stem: String,
    /// File the analysis reads. RAW when there is one (its embedded JPEG is
    /// the camera's full-quality rendering), otherwise the image file.
    pub primary: SourceFile,
    /// Other files of the same photo (JPEG twin of a RAW, …).
    pub companions: Vec<SourceFile>,
    /// Existing `.xmp` sidecar, if any (we never overwrite fields we don't own).
    pub xmp_sidecar: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExifInfo {
    /// DateTimeOriginal (+ SubSecTimeOriginal), read as UTC, Unix ms.
    pub captured_at_ms: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    /// Body serial number: separates two bodies of the same model when
    /// a wedding is shot with two cameras.
    pub camera_serial: Option<String>,
    pub lens: Option<String>,
    pub iso: Option<u32>,
    /// Exposure time in seconds (1/250 s → 0.004).
    pub exposure_time_s: Option<f32>,
    pub f_number: Option<f32>,
    pub focal_length_mm: Option<f32>,
    /// EXIF orientation 1..=8 (1 = upright).
    pub orientation: u8,
    /// Dimensions of the full-resolution image (sensor output), upright.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Rating already written by the camera or another tool (`xmp:Rating`,
    /// -1 = rejected, 0..=5 stars).
    pub rating: Option<i8>,
    pub gps: Option<GpsPosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpsPosition {
    pub latitude: f64,
    pub longitude: f64,
}

// ---------------------------------------------------------------------------
// Technical metrics (cull-analysis)
// ---------------------------------------------------------------------------

/// Sharpness measures. Raw values are variance-of-Laplacian style energies
/// computed on a 1024 px long-edge grayscale version, so they are comparable
/// between photos of the same project.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharpnessMetrics {
    /// Whole-frame energy.
    pub global: f32,
    /// Energy of the sharpest region (high percentile of a tile grid): the
    /// in-focus subject of a shallow depth-of-field portrait.
    pub peak: f32,
    /// Energy measured on the main subject (largest face/eyes) when known.
    pub subject: Option<f32>,
    /// 0 = no directional blur, 1 = strong motion blur / camera shake.
    pub motion_blur: f32,
    /// Normalised focus score in `[0, 1]` (1 = tack sharp).
    pub score: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposureMetrics {
    /// Mean luminance in `[0, 1]` (Rec. 709 luma).
    pub mean_luma: f32,
    /// Median luminance in `[0, 1]`.
    pub median_luma: f32,
    /// Fraction of pixels with luma >= 250/255.
    pub highlights_clipped: f32,
    /// Fraction of pixels with luma <= 5/255.
    pub shadows_clipped: f32,
    /// RMS contrast in `[0, 1]`.
    pub contrast: f32,
    /// Normalised exposure quality in `[0, 1]` (1 = well exposed).
    pub score: f32,
}

/// Perceptual hashes (64 bits each) used for exact/near duplicates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageHashes {
    /// DCT perceptual hash.
    pub phash: u64,
    /// Gradient (difference) hash.
    pub dhash: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TechnicalMetrics {
    pub sharpness: SharpnessMetrics,
    pub exposure: ExposureMetrics,
    pub hashes: ImageHashes,
}

// ---------------------------------------------------------------------------
// ML observations (cull-ml)
// ---------------------------------------------------------------------------

/// Axis-aligned rectangle, normalised coordinates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FaceObservation {
    pub bbox: NormRect,
    /// Detector confidence in `[0, 1]`.
    pub confidence: f32,
    /// Right eye, left eye, nose tip, right mouth corner, left mouth corner
    /// (YuNet order, from the subject's point of view).
    pub landmarks: [NormPoint; 5],
    /// Probability that each eye is open (`None` when the crop was too small
    /// or the eye-state model is unavailable).
    pub right_eye_open: Option<f32>,
    pub left_eye_open: Option<f32>,
    /// Focus score in `[0, 1]` of the sharpest eye, measured at a fixed
    /// inter-ocular distance (filled by the engine with cull-analysis; the
    /// central face region when the eyes cannot be placed).
    pub sharpness: Option<f32>,
    /// Face area / image area.
    pub area_ratio: f32,
    /// Probability in `[0, 1]` that the face mesh recognised a face in this
    /// box. `None` when the mesh did not run on it (face too small, model
    /// missing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh_presence: Option<f32>,
    /// Head orientation and mouth opening read from the face mesh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pose: Option<HeadPose>,
    /// Per-face expression scores (blink, squint, smile…) from a dedicated
    /// model, when one ran. Preferred over the eye aspect ratio for the eye
    /// state when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<FaceExpression>,
    /// Rec. 709 luma in `[0, 1]` of the face skin (65th percentile of the
    /// face core, so glasses or a beard do not darken it), and the share of
    /// its pixels blown out (≥ 250/255): the subject's exposure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin_luma: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin_clipped: Option<f32>,
    /// 64-bit gradient hash of the person around this face (head, shoulders,
    /// arms), anchored on the face: two frames of the same pose share it, a
    /// hand moved to the hair or the pockets changes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_hash: Option<u64>,
}

/// Head orientation in degrees (0 = facing the camera, upright) and mouth
/// opening, from the face mesh landmarks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadPose {
    /// Turn left/right (± 90 = profile).
    pub yaw: f32,
    /// Nod: positive = looking down.
    pub pitch: f32,
    /// Tilt of the eye line in the image.
    pub roll: f32,
    /// Inner-lip opening / mouth width (≈ 0 closed, ≥ 0.3 laughing).
    pub mouth_open: f32,
}

/// Expression scores in `[0, 1]` of one face (ARKit blendshape names), from
/// an expression model such as MediaPipe's face blendshapes. Every field is
/// optional: a model fills what it knows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FaceExpression {
    pub eye_blink_right: Option<f32>,
    pub eye_blink_left: Option<f32>,
    pub eye_squint_right: Option<f32>,
    pub eye_squint_left: Option<f32>,
    pub smile: Option<f32>,
    pub jaw_open: Option<f32>,
}

impl FaceObservation {
    /// Both eyes known and at least one below `threshold`.
    pub fn eyes_closed(&self, threshold: f32) -> Option<bool> {
        match (self.right_eye_open, self.left_eye_open) {
            (Some(r), Some(l)) => Some(r < threshold && l < threshold),
            _ => None,
        }
    }
}

/// What a subject region was found as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubjectKind {
    Person,
    Animal,
}

/// Pixel mask of a subject region, at the model's resolution, stretched over
/// the whole upright photo (row-major, `true` = subject). Only lives during
/// the analysis of the photo: it is not stored.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubjectMask {
    pub width: u32,
    pub height: u32,
    pub data: Vec<bool>,
}

impl SubjectMask {
    /// Is normalised point (x, y) in the mask?
    pub fn contains(&self, x: f32, y: f32) -> bool {
        if self.width == 0 || self.height == 0 || !(x.is_finite() && y.is_finite()) {
            return false;
        }
        let i = ((x * self.width as f32) as u32).min(self.width - 1);
        let j = ((y * self.height as f32) as u32).min(self.height - 1);
        self.data.get((j * self.width + i) as usize).copied().unwrap_or(false)
    }
}

/// The main subject of a photo without a usable face (a person seen from the
/// back, an animal), from an instance segmentation model: the region whose
/// focus and exposure judge the photo instead of the whole frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubjectObservation {
    pub kind: SubjectKind,
    /// Detector score in `[0, 1]`.
    pub score: f32,
    /// Bounding box of the mask.
    pub bbox: NormRect,
    /// Share of the frame covered by the mask.
    pub area_ratio: f32,
    /// Focus score in `[0, 1]` of the region (filled by the engine with
    /// cull-analysis).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sharpness: Option<f32>,
    /// Stops of overexposure of the masked pixels, read like the frame's
    /// (filled by the engine with cull-analysis).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ev_over: Option<f32>,
    #[serde(skip)]
    pub mask: Option<SubjectMask>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MlObservations {
    pub faces: Vec<FaceObservation>,
    /// L2-normalised image embedding used to group similar shots (CLIP
    /// ViT-B/32 in Standard precision, DINOv2 in Maximum). Not serialised to
    /// the UI: the engine stores it as a BLOB.
    #[serde(skip)]
    pub embedding: Option<Vec<f32>>,
    /// L2-normalised CLIP embedding for text search when it is not
    /// `embedding` (CLIP ViT-L/14 in Maximum precision). Stored as a BLOB.
    #[serde(skip)]
    pub search_embedding: Option<Vec<f32>>,
    /// Aesthetic score, LAION scale (roughly 1..10).
    pub aesthetic: Option<f32>,
    /// The subject region when the photo has no usable face (Maximum).
    pub subject: Option<SubjectObservation>,
    /// Identifier of the models that produced this, e.g.
    /// `yunet-2023mar+facemesh-v2-ear3+blendshapes-v2-eye1+clip-b32-int8-lb+laion-linear-b32`.
    pub models: String,
}

/// Which models analyse a photo: those shipped with the app, or the
/// optional Maximum pack (bigger models, slower, finer results).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Precision {
    #[default]
    Standard,
    Maximum,
}

// ---------------------------------------------------------------------------
// Decisions
// ---------------------------------------------------------------------------

/// Automatic verdict of the AI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Selected (best of its group, no blocking defect).
    Keep,
    /// Acceptable but not the best, or borderline: the photographer decides.
    Maybe,
    /// Technical defect or inferior duplicate.
    Reject,
}

/// Why a photo was rated down. Ordered from most to least severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// Whole frame out of focus.
    Blurry,
    /// Camera shake / subject motion.
    MotionBlur,
    /// Main subject's face not sharp while the frame is.
    SubjectOutOfFocus,
    /// Main subject (or most faces) eyes closed.
    EyesClosed,
    /// Very dark / crushed shadows.
    Underexposed,
    /// Very bright / blown highlights.
    Overexposed,
    /// A better shot of the same moment exists.
    Duplicate,
    /// Near-identical copy (same file re-exported, burst at 0 ms).
    ExactDuplicate,
    /// Low aesthetic score (composition/light) relative to the shoot.
    LowAesthetic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Assessment {
    /// Overall quality score 0..100.
    pub score: f32,
    pub verdict: Verdict,
    /// Sorted by severity, deduplicated.
    pub reasons: Vec<Reason>,
    /// Similar-shots group this photo belongs to (`None` = alone).
    pub group_id: Option<u32>,
    /// 0 = best of its group.
    pub group_rank: u32,
    /// Sub-scores in `[0, 1]` shown in the inspector.
    pub breakdown: ScoreBreakdown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreBreakdown {
    pub focus: f32,
    pub exposure: f32,
    /// `None` when there are no faces.
    pub eyes: Option<f32>,
    /// `None` when the aesthetic model did not run.
    pub aesthetic: Option<f32>,
    /// Uniqueness within the shoot (1 = unique moment).
    pub uniqueness: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GroupKind {
    /// Shots taken within a burst window and visually near-identical.
    Burst,
    /// Same subject/framing, not necessarily consecutive.
    Similar,
    /// Byte-identical or hash-identical copies.
    Exact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarGroup {
    pub id: u32,
    pub kind: GroupKind,
    /// Photo ids, best first.
    pub photo_ids: Vec<String>,
}

/// A moment of the shoot (ceremony, cocktail…), split on capture-time gaps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub id: u32,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    pub photo_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Sensitivity {
    /// Rejects more aggressively (event coverage, thousands of frames).
    Strict,
    #[default]
    Balanced,
    /// Rejects only obvious defects (artistic work, intentional blur).
    Lenient,
}

/// What the culling looks for, per project (and the app's defaults for new
/// projects). One switch per [`Reason`] the photographer can turn off; a
/// detection turned off never produces its reason (cull-analysis `score`
/// documents the exact rules).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CullSettings {
    pub sensitivity: Sensitivity,
    /// Whole frame out of focus ([`Reason::Blurry`]).
    pub detect_blur: bool,
    /// Camera shake or subject motion ([`Reason::MotionBlur`]).
    pub detect_motion_blur: bool,
    /// Main face soft while something else is sharp
    /// ([`Reason::SubjectOutOfFocus`]).
    pub detect_subject_focus: bool,
    /// Respect artistic blur (bokeh, shallow depth of field, blur on
    /// purpose): frame-level blur only rejects a photo when nothing in it is
    /// sharp, clearly soft frames go to review, and
    /// [`Reason::SubjectOutOfFocus`] is not raised. See cull-analysis `score`.
    pub artistic_blur: bool,
    /// [`Reason::EyesClosed`].
    pub detect_eyes_closed: bool,
    /// [`Reason::Underexposed`].
    pub detect_underexposed: bool,
    /// [`Reason::Overexposed`].
    pub detect_overexposed: bool,
    /// Keep only the best (`keep_per_group`) of each group of similar shots
    /// or burst: the others go to review or are rejected
    /// ([`Reason::Duplicate`]). Groups are formed either way, so the review
    /// can stack them.
    pub group_similar: bool,
    /// How many photos are kept per group of similar shots (1..=5).
    pub keep_per_group: u8,
    /// Reject copies of the same image ([`Reason::ExactDuplicate`]). Off,
    /// copies are ordinary similar shots.
    pub detect_exact_duplicates: bool,
    /// Maximum time between two shots of the same burst/group, seconds.
    pub group_window_s: f32,
    /// Cosine similarity of embeddings above which two shots are "similar"
    /// (0.80 loose … 0.97 near-identical).
    pub similarity_threshold: f32,
    /// Gap between two scenes, seconds.
    pub scene_gap_s: f32,
}

impl Default for CullSettings {
    fn default() -> Self {
        CullSettings {
            sensitivity: Sensitivity::Balanced,
            detect_blur: true,
            detect_motion_blur: true,
            detect_subject_focus: true,
            artistic_blur: false,
            detect_eyes_closed: true,
            detect_underexposed: true,
            detect_overexposed: true,
            group_similar: true,
            keep_per_group: 1,
            detect_exact_duplicates: true,
            group_window_s: 45.0,
            similarity_threshold: 0.90,
            scene_gap_s: 15.0 * 60.0,
        }
    }
}

impl CullSettings {
    /// At least one of the three blur detections is on.
    pub fn checks_sharpness(&self) -> bool {
        self.detect_blur || self.detect_motion_blur || self.detect_subject_focus
    }
}

// ---------------------------------------------------------------------------
// Photographer's own marks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Flag {
    #[default]
    None,
    Pick,
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ColorLabel {
    #[default]
    None,
    Red,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl ColorLabel {
    /// Name written in `xmp:Label` (Lightroom / Bridge default label set).
    pub fn xmp_name(self) -> Option<&'static str> {
        match self {
            ColorLabel::None => None,
            ColorLabel::Red => Some("Red"),
            ColorLabel::Yellow => Some("Yellow"),
            ColorLabel::Green => Some("Green"),
            ColorLabel::Blue => Some("Blue"),
            ColorLabel::Purple => Some("Purple"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UserMark {
    pub flag: Flag,
    /// 0..=5 stars.
    pub rating: u8,
    pub label: ColorLabel,
}

/// Final state of a photo after the photographer's review: their flag wins
/// over the AI verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FinalDecision {
    Selected,
    ToReview,
    Rejected,
}

pub fn final_decision(ai: Option<Verdict>, mark: &UserMark) -> FinalDecision {
    match mark.flag {
        Flag::Pick => FinalDecision::Selected,
        Flag::Reject => FinalDecision::Rejected,
        Flag::None => match ai {
            Some(Verdict::Keep) => FinalDecision::Selected,
            Some(Verdict::Reject) => FinalDecision::Rejected,
            Some(Verdict::Maybe) | None => FinalDecision::ToReview,
        },
    }
}

// ---------------------------------------------------------------------------
// Progress
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Scanning,
    /// Loading the Maximum pack's models, before the first photo.
    Loading,
    Analyzing,
    Grouping,
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: Stage,
    pub done: u32,
    pub total: u32,
    pub current: Option<String>,
    pub photos_per_second: f32,
    pub eta_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions() {
        assert_eq!(FileKind::from_extension("CR3"), Some(FileKind::Raw));
        assert_eq!(FileKind::from_extension("jpeg"), Some(FileKind::Jpeg));
        assert_eq!(FileKind::from_extension("xmp"), None);
    }

    #[test]
    fn user_flag_wins() {
        let pick = UserMark { flag: Flag::Pick, ..Default::default() };
        assert_eq!(final_decision(Some(Verdict::Reject), &pick), FinalDecision::Selected);
        let none = UserMark::default();
        assert_eq!(final_decision(Some(Verdict::Maybe), &none), FinalDecision::ToReview);
    }

    #[test]
    fn camel_case() {
        let s = serde_json::to_string(&Reason::SubjectOutOfFocus).unwrap();
        assert_eq!(s, "\"subjectOutOfFocus\"");
    }

    #[test]
    fn settings_round_trip() {
        let s =
            CullSettings { artistic_blur: true, detect_overexposed: false, keep_per_group: 3, ..Default::default() };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["artisticBlur"], true);
        assert_eq!(json["detectExactDuplicates"], true);
        assert!(json.get("detectExposure").is_none());
        assert_eq!(serde_json::from_value::<CullSettings>(json).unwrap(), s);
    }
}
