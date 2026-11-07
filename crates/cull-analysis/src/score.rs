//! Scoring and verdicts: the decision logic of Pelli Instant.
//!
//! Everything here is a pure function of the per-photo facts, the groups and
//! the settings, cheap enough (≈1 ms for 5 000 photos) to be re-run live when
//! the photographer moves the sensitivity slider. The score itself does not
//! depend on the sensitivity, only the verdict thresholds do, so the ranking
//! inside a group never changes when the slider moves.
//!
//! # 0. The subject
//!
//! The **main face** is the largest face detected with confidence ≥ 0.8
//! (`MAIN_FACE_CONFIDENCE`) **that the face mesh recognises as a face**
//! (presence ≥ 0.5), at least 0.15% of the frame. YuNet boxes a bicycle part
//! or a hand at 0.6-0.75 now and then; the mesh does not see a face there.
//! When the mesh never ran on the shoot (model missing),
//! the confidence alone decides. Without a main face the photo is judged on
//! the frame only.
//!
//! # 1. Sub-scores (all in `[0, 1]`, shown in the inspector)
//!
//! * **focus** — the frame focus score (sharpest tiles of the frame). With a
//!   main face it is `0.6 × face + 0.4 × frame`, the face focus being that of
//!   its sharpest eye at a fixed inter-ocular distance (`eye_sharpness`).
//!   Then reduced by up to 50% for motion blur (`× (1 − 0.5 × motion_blur)`,
//!   not when motion blur detection is off).
//! * **exposure** — the exposure score (stops off, tolerant of low/high
//!   key). With a main face covering ≥ 0.3% of the frame it is judged on the
//!   face **skin**, the frame counting at most one stop beyond it
//!   (`exposure::with_skin`): a backlit portrait on a blown sky, a lit face in
//!   a dark workshop are well exposed. A face the skin finds badly exposed
//!   in a frame that is not (shade, sunglasses, a blown face) goes to review
//!   at most: rejecting needs the frame's own reading to block too.
//! * **eyes** — openness of the main face's eyes: the **less open** of both
//!   eyes when both are visible (a laugh closes both; one eye misread as open
//!   must not win), as read by cull-ml (blendshapes, else eye aspect ratio). In
//!   group photos (≥ 3 faces at least a quarter of the main face's size) it
//!   is `0.5 × main openness + 0.5 × share of faces with open eyes`. `None`
//!   without faces or eye data.
//! * **aesthetic** — the LAION aesthetic score made relative to the shoot:
//!   `z = (a − median) / spread` (spread = 1.48 × MAD, at least 0.35), mapped
//!   through a logistic (`0.5` = the shoot's median photo). On small shoots
//!   (< ~20 photos) it is blended with an absolute scale centred on 5.0.
//! * **uniqueness** — `1 / √n` for a photo in a group of `n` similar shots,
//!   1 for a unique moment.
//!
//! # 2. Overall score (0..100)
//!
//! Weighted mean of the available sub-scores: focus 0.50, exposure 0.20,
//! eyes 0.15, uniqueness 0.05, and the aesthetic, whose weight is set so it
//! makes exactly **10%** of the mean whatever else is available (generic
//! "pretty picture" scores prefer landscapes to product details and calm
//! faces to laughter: a tie-breaker, not a judge). A detection disabled in
//! the settings removes its sub-score from the mean: focus when the three
//! blur detections are off, exposure when the detection of the photo's
//! direction is off, eyes when closed-eye detection is off. Technical
//! failures cannot be compensated by a nice composition: when the weaker of
//! focus and exposure is below 0.5, the mean is multiplied by
//! `0.6 + 0.8 × weakest`. Eyes and aesthetic are not part of this lock: a
//! laugh with squinted eyes is not a technical failure.
//!
//! # 3. Defects
//!
//! Each technical quality `q` (frame focus, main face focus, exposure) is
//! judged both **absolutely** and **relative to the shoot**: the shoot
//! reference is the 75th percentile of `q` over the project (blended with a
//! prior on small projects), i.e. "what a good frame looks like with this
//! camera, lens and light". With the thresholds of the table below:
//!
//! * **blocking** (Reject) when `q < obvious`, or when `q < abs` *and*
//!   `q < rel × reference`;
//! * **soft** (Maybe) when `q < soft_abs` *and* `q < soft_rel × reference`.
//!
//! The relative test is what keeps a whole dim reception or a vintage-lens
//! session from being rejected, while a frame far below its siblings is.
//!
//! | quality (reason)                 | | Strict | Balanced | Lenient |
//! |----------------------------------|-|--------|----------|---------|
//! | frame focus (`Blurry`)           | obvious / abs / rel | 0.20 / 0.50 / 0.75 | 0.12 / 0.30 / 0.60 | 0.05 / 0.12 / 0.40 |
//! |                                  | soft abs / rel      | 0.70 / 0.88 | 0.50 / 0.80 | 0.25 / 0.55 |
//! | face focus (`SubjectOutOfFocus`) | obvious / abs / rel | 0.15 / 0.40 / 0.70 | 0.08 / 0.28 / 0.55 | 0.03 / 0.12 / 0.35 |
//! |                                  | soft abs / rel      | 0.55 / 0.85 | 0.42 / 0.72 | 0.22 / 0.50 |
//! | exposure (`Under/Overexposed`)   | obvious / abs / rel | 0.15 / 0.40 / 0.65 | 0.06 / 0.25 / 0.50 | 0.02 / 0.08 / 0.30 |
//! |                                  | soft abs / rel      | 0.65 / 0.80 | 0.50 / 0.70 | 0.25 / 0.50 |
//! | motion blur (`MotionBlur`)       | soft / blocking / obvious | 0.30 / 0.50 / 0.85 | 0.40 / 0.65 / 0.90 | 0.60 / 0.85 / 0.97 |
//! | eyes: openness below = closed    | | 0.50 | 0.35 | 0.20 |
//! | eyes closed on a portrait        | | Reject (soft: Maybe below 0.65) | not flagged | not flagged |
//! | eyes: share of closed faces (group photo) soft / blocking | | > 0 / 0.15 | 0.15 / (Maybe) 0.30 | 0.30 / (Maybe) 0.50 |
//! | Maybe margin inside a burst (points) | | 2 | 4 | 7 |
//!
//! Notes:
//! * **A sharp main face wins.** When the main face covers ≥ 0.5% of the
//!   frame and its focus is ≥ 0.75, the subject is in focus: frame-level
//!   `Blurry` and `MotionBlur` are not raised and motion does not lower the
//!   focus sub-score (long hair, fur or a textured wall can look
//!   directional; a shallow depth of field can leave little sharp area).
//!   Under motion at the sensitivity's blocking level, the face must be
//!   ≥ 0.9 to overrule it: camera shake smears the eyes across the movement,
//!   but the lids along it stay sharp enough to read 0.8. Between 0.75 and
//!   0.9 the frame-level defects only send the photo to review (flash
//!   ghosting, a moving hand).
//! * `SubjectOutOfFocus` is only raised when the frame itself is not blurry,
//!   on a main face (confirmed by the mesh), and never when **another face**
//!   of the photo is sharp (a blurred foreground figure, the subject sharp
//!   behind) or when the **head is turned away or bent** (`|yaw| > 50°`,
//!   `|pitch| > 25°`, `|roll| > 35°`: looking down at the work, a profile,
//!   eyes out of view); it can only block when the main face covers ≥ 1.2%
//!   of the frame (a smaller soft face may be a figure behind the subject
//!   seen from the back: the photo goes to review).
//! * Eyes are only read on faces covering at least `EYES_MIN_AREA` (0.3%) of
//!   the frame: smaller ones never raise `EyesClosed` nor count in the eyes
//!   sub-score.
//! * `EyesClosed`: on a **group photo** (≥ 3 comparable faces), the share of
//!   faces with closed eyes, on every sensitivity (Maybe only, except on
//!   Strict). On a **portrait**, only Strict flags it (eyes closed, or almost
//!   closed: below the threshold + 0.15): a kiss, a prayer, a look down at
//!   the bouquet, a laugh or the sun close the eyes on purpose, and the eye
//!   model cannot tell. In a burst the frame with open eyes wins through the
//!   score (eyes are 15% of it); a frame left out of its group with its eyes
//!   closed carries `EyesClosed` next to `Duplicate`, as an explanation.
//! * Exposure direction: `Overexposed` when the photo reads bright (the
//!   skin's over-reading wins, or the frame's mean luma ≥ 0.5), otherwise
//!   `Underexposed`.
//! * A detection disabled in the settings never produces its reason. Each
//!   softness is first attributed to one cause (motion, frame defocus, the
//!   subject), then dropped if that cause's detection is off: with motion blur
//!   detection off, a frame softened by motion is not called `Blurry`
//!   instead, and with frame blur detection off, a blurry frame is not called
//!   `SubjectOutOfFocus` instead.
//!
//! ## Artistic blur (`artistic_blur`)
//!
//! For photographers who use a shallow depth of field, bokeh, a blurred
//! foreground or blur on purpose: a soft area must not count against a photo.
//! The frame score is that of its sharpest tiles (`PEAK_TILES`), so a
//! very small sharp subject on bokeh can still read as soft; and the face
//! focus measure, even on the eyes, cannot tell a focus missed by accident
//! from a sharp plane chosen elsewhere. The option is therefore conservative
//! and uses the frame only:
//!
//! * frame-level `Blurry` and `MotionBlur` reject only at the `obvious`
//!   level of the table (nothing in the frame is sharp, or strong shake); a
//!   blocking level only sends the photo to review; a soft level is not
//!   flagged at all;
//! * `SubjectOutOfFocus` is never raised (whatever its own switch says);
//! * scores are unchanged: the face still weighs in the focus sub-score as
//!   above, and a sharp main face still overrides the frame-level measures
//!   as without the option.
//!
//! Limits: a photo whose only sharp spot is very small still goes to review
//! when its frame score is blocking; a truly missed focus with a sharp
//! background is no longer flagged (that is what `SubjectOutOfFocus` would
//! catch); a soft photo made on purpose (dreamy effect, intentional camera
//! movement) is still rejected when nothing in it is sharp: for such work,
//! turn the blur detections off.
//!
//! # 4. Verdicts
//!
//! 1. **Exact copies** (identical pHash and dHash with the same focus score
//!    and mean luma, or an `Exact` group): all but the best copy are
//!    `Reject` + `ExactDuplicate`. With exact duplicate detection off, copies
//!    are judged like any other photo (and, grouping on, as similar shots).
//! 2. **Groups** (when grouping is on), members ranked by score: the
//!    `keep_per_group` best members without blocking defect are `Keep`
//!    (`Maybe` if they carry a soft defect); members with a blocking defect
//!    are `Reject` with its reasons. The other defect-free members carry
//!    `Duplicate`:
//!    * in a **burst** (one moment, frames ≤ 2 s apart): `Maybe` when within
//!      the margin of the last kept one, otherwise `Reject`;
//!    * in a **series of similar shots** (`Similar`: poses, expressions of
//!      one set-up) they are never rejected: `Maybe`, stacked under the kept
//!      ones (at least one in five is kept). A series is the photographer's
//!      choice of expressions; only a burst is a true duplicate.
//!
//!    If *no* member is defect-free, the best one without an obvious defect
//!    becomes `Maybe` (keeping its reasons), so a moment is never silently
//!    thrown away.
//! 3. **Singles**: blocking defect → `Reject`; soft defect → `Maybe`;
//!    otherwise `Keep`. The aesthetic score only ranks: `LowAesthetic` is no
//!    longer raised (on delivered shoots it flagged product details,
//!    changing rooms and laughs: the model has no idea what the shoot is
//!    for).
//!
//! On a typical wedding (half the frames in bursts of 3-6) Balanced keeps
//! a little under half of the photos (≈42% on the simulated wedding of
//! the tests; it was a third before closed eyes on portraits and a low
//! aesthetic stopped sending photos to review).

use crate::exposure;
use crate::hash::{exact_copy_sets, CopyKey};
use crate::PhotoFacts;
use cull_types::{
    Assessment, CullSettings, FaceObservation, GroupKind, Reason, ScoreBreakdown, Sensitivity, SimilarGroup, Verdict,
};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Constants (see the module documentation)
// ---------------------------------------------------------------------------

const FACE_MIN_CONFIDENCE: f32 = 0.5;
/// The main face (the subject) is the largest face detected with at least
/// this confidence and recognised by the face mesh.
pub(crate) const MAIN_FACE_CONFIDENCE: f32 = 0.8;
/// Face-mesh presence from which a detection is a face.
pub(crate) const MESH_PRESENCE: f32 = 0.5;
pub(crate) const MAIN_FACE_MIN_AREA: f32 = 0.0015;
const FACE_RELIABLE_AREA: f32 = 0.005;
/// `SubjectOutOfFocus` can only reject from this face size (share of the
/// frame): below, a soft face may be a figure behind the subject, and the
/// photo goes to review.
const SUBJECT_BLOCK_AREA: f32 = 0.012;
/// A main face measured at least this sharp (and at least
/// `FACE_RELIABLE_AREA` of the frame) proves the subject is in focus: the
/// frame-level blur and motion measures are then ignored (hair, fur or a
/// textured wall can look directional; a shallow depth of field can leave
/// little sharp area). Another face this sharp vetoes `SubjectOutOfFocus`.
const SUBJECT_SHARP_FACE: f32 = 0.75;
/// Strong frame-level motion (the sensitivity's blocking level) is only
/// overruled by a face at least this sharp: camera shake smears the eyes
/// across the movement, but an eye band keeps sharp lids along it.
const SUBJECT_SHARP_UNDER_MOTION: f32 = 0.9;
/// Head orientations (degrees) beyond which the face focus says nothing
/// about the focus: looking down at the work, a profile, a strong tilt.
const HEAD_AWAY_YAW: f32 = 50.0;
const HEAD_AWAY_PITCH: f32 = 25.0;
const HEAD_AWAY_ROLL: f32 = 35.0;
const COMPARABLE_FACE_RATIO: f32 = 0.25;
/// Smallest face (share of the frame) whose eyes and skin are judged.
const EYES_MIN_AREA: f32 = 0.003;
/// Face-mesh presence from which the skin of the main face judges the
/// exposure.
const SKIN_MESH_PRESENCE: f32 = 0.9;
/// Smallest subject region (share of the frame) that judges a photo without
/// a usable face.
const SUBJECT_MIN_AREA: f32 = 0.01;
/// In a `Similar` group (not a burst), keep at least one photo per this many.
const SERIES_KEEP_EVERY: usize = 5;
const GROUP_PHOTO_MIN_FACES: usize = 3;
const EYES_SCORE_CLOSED: f32 = 0.35;
const EYES_SOFT_MARGIN: f32 = 0.15;

const W_FOCUS: f32 = 0.50;
const W_EXPOSURE: f32 = 0.20;
const W_EYES: f32 = 0.15;
const W_UNIQUENESS: f32 = 0.05;
/// Share of the mean given to the aesthetic sub-score when there is one.
const AESTHETIC_SHARE: f32 = 0.10;
const FACE_FOCUS_WEIGHT: f32 = 0.6;
const MOTION_FOCUS_PENALTY: f32 = 0.5;
const GATE_BELOW: f32 = 0.5;
const GATE_FLOOR: f32 = 0.6;

const REF_PERCENTILE: f32 = 0.75;
const REF_PRIOR_WEIGHT: f32 = 12.0;
const FOCUS_REF_PRIOR: f32 = 0.85;
const FACE_REF_PRIOR: f32 = 0.85;
const EXPOSURE_REF_PRIOR: f32 = 0.90;

const AES_SPREAD_MIN: f32 = 0.35;
const AES_SLOPE: f32 = 1.3;
const AES_ABS_CENTER: f32 = 5.0;
const AES_ABS_SPREAD: f32 = 0.9;
const AES_PRIOR_WEIGHT: f32 = 8.0;

// ---------------------------------------------------------------------------
// Sensitivity parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Rule {
    obvious: f32,
    abs: f32,
    rel: f32,
    soft_abs: f32,
    soft_rel: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    None,
    Soft,
    Block,
    Obvious,
}

impl Rule {
    fn level(&self, q: f32, reference: f32) -> Level {
        if q < self.obvious {
            Level::Obvious
        } else if q < self.abs && q < self.rel * reference {
            Level::Block
        } else if q < self.soft_abs && q < self.soft_rel * reference {
            Level::Soft
        } else {
            Level::None
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Params {
    focus: Rule,
    face: Rule,
    exposure: Rule,
    motion_soft: f32,
    motion_block: f32,
    motion_obvious: f32,
    eyes_closed_below: f32,
    eyes_group_soft: f32,
    eyes_group_block: f32,
    eyes_can_reject: bool,
    /// Closed eyes on a portrait (fewer than 3 faces) are flagged.
    eyes_on_portraits: bool,
    group_margin: f32,
}

impl Params {
    fn new(s: Sensitivity) -> Params {
        match s {
            Sensitivity::Strict => Params {
                focus: Rule { obvious: 0.20, abs: 0.50, rel: 0.75, soft_abs: 0.70, soft_rel: 0.88 },
                face: Rule { obvious: 0.15, abs: 0.40, rel: 0.70, soft_abs: 0.55, soft_rel: 0.85 },
                exposure: Rule { obvious: 0.15, abs: 0.40, rel: 0.65, soft_abs: 0.65, soft_rel: 0.80 },
                motion_soft: 0.30,
                motion_block: 0.50,
                motion_obvious: 0.85,
                eyes_closed_below: 0.50,
                eyes_group_soft: 1e-6,
                eyes_group_block: 0.15,
                eyes_can_reject: true,
                eyes_on_portraits: true,
                group_margin: 2.0,
            },
            Sensitivity::Balanced => Params {
                focus: Rule { obvious: 0.12, abs: 0.30, rel: 0.60, soft_abs: 0.50, soft_rel: 0.80 },
                face: Rule { obvious: 0.08, abs: 0.28, rel: 0.55, soft_abs: 0.42, soft_rel: 0.72 },
                exposure: Rule { obvious: 0.06, abs: 0.25, rel: 0.50, soft_abs: 0.50, soft_rel: 0.70 },
                motion_soft: 0.40,
                motion_block: 0.65,
                motion_obvious: 0.90,
                eyes_closed_below: 0.35,
                eyes_group_soft: 0.15,
                eyes_group_block: 0.30,
                eyes_can_reject: false,
                eyes_on_portraits: false,
                group_margin: 4.0,
            },
            Sensitivity::Lenient => Params {
                focus: Rule { obvious: 0.05, abs: 0.12, rel: 0.40, soft_abs: 0.25, soft_rel: 0.55 },
                face: Rule { obvious: 0.03, abs: 0.12, rel: 0.35, soft_abs: 0.22, soft_rel: 0.50 },
                exposure: Rule { obvious: 0.02, abs: 0.08, rel: 0.30, soft_abs: 0.25, soft_rel: 0.50 },
                motion_soft: 0.60,
                motion_block: 0.85,
                motion_obvious: 0.97,
                eyes_closed_below: 0.20,
                eyes_group_soft: 0.30,
                eyes_group_block: 0.50,
                eyes_can_reject: false,
                eyes_on_portraits: false,
                group_margin: 7.0,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Per-photo features (sensitivity independent)
// ---------------------------------------------------------------------------

fn finite_or(v: f32, default: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        default
    }
}

fn unit(v: f32, default: f32) -> f32 {
    finite_or(v, default).clamp(0.0, 1.0)
}

fn clean(v: Option<f32>) -> Option<f32> {
    v.filter(|x| x.is_finite()).map(|x| x.clamp(0.0, 1.0))
}

/// Head turn and downward nod (degrees) from which the less open eye's
/// reading is not trusted (a foreshortened far eye, lids lowered over a
/// downcast look read closed).
const TURNED_HEAD_YAW: f32 = 25.0;
const LOOKING_DOWN_PITCH: f32 = 15.0;

/// Openness of a face's eyes: the less open of the two when both are known
/// and the head faces the camera (a laugh closes both; an eye misread as open
/// must not hide a closed one), the more open one when the head is turned or
/// looks down (the far eye is foreshortened, lowered lids read closed). The
/// probabilities come from cull-ml, which reads them on the blendshapes
/// (blink, squint, smile) when that model ran, on the eye aspect ratio
/// otherwise.
pub(crate) fn openness(f: &FaceObservation) -> Option<f32> {
    let (r, l) = (clean(f.right_eye_open), clean(f.left_eye_open));
    let turned = f.pose.is_some_and(|p| {
        (p.yaw.is_finite() && p.yaw.abs() >= TURNED_HEAD_YAW) || (p.pitch.is_finite() && p.pitch >= LOOKING_DOWN_PITCH)
    });
    match (r, l) {
        (Some(r), Some(l)) if turned => Some(r.max(l)),
        (Some(r), Some(l)) => Some(r.min(l)),
        (Some(v), None) | (None, Some(v)) => Some(v),
        (None, None) => None,
    }
}

/// The shoot's faces went through the face mesh (it can confirm faces).
pub(crate) fn mesh_ran(photos: &[PhotoFacts<'_>]) -> bool {
    photos.iter().any(|p| p.faces.iter().any(|f| f.mesh_presence.is_some()))
}

/// A detection that can be a subject: confident, and a face for the mesh.
fn is_subject(f: &FaceObservation, mesh: bool) -> bool {
    usable(f)
        && f.confidence >= MAIN_FACE_CONFIDENCE
        && match f.mesh_presence {
            Some(p) => p.is_finite() && p >= MESH_PRESENCE,
            None => !mesh,
        }
}

fn usable(f: &FaceObservation) -> bool {
    f.confidence.is_finite() && f.confidence >= FACE_MIN_CONFIDENCE && f.area_ratio.is_finite() && f.area_ratio > 0.0
}

/// The main face: the largest subject face, big enough to judge.
pub(crate) fn main_face(faces: &[FaceObservation], mesh: bool) -> Option<&FaceObservation> {
    faces
        .iter()
        .filter(|f| is_subject(f, mesh))
        .max_by(|a, b| a.area_ratio.total_cmp(&b.area_ratio))
        .filter(|f| f.area_ratio >= MAIN_FACE_MIN_AREA)
}

#[derive(Debug, Clone)]
struct Features {
    frame_focus: f32,
    motion: f32,
    /// Main face: (area ratio, focus).
    face_area: f32,
    face_focus: Option<f32>,
    /// Another subject face of the photo is sharp.
    other_face_sharp: bool,
    /// The main face looks away, down or is strongly tilted.
    head_away: bool,
    /// Focus of the subject region of a photo without a main face.
    subject_focus: Option<f32>,
    /// Openness of the main face (significant faces only).
    main_open: Option<f32>,
    /// Openness of comparable faces when this is a group photo.
    group_open: Vec<f32>,
    exposure: f32,
    bright: bool,
    /// The frame's own exposure score when the face skin judged the
    /// exposure (`None` otherwise).
    frame_exposure: Option<f32>,
    aesthetic: Option<f32>,
    time: Option<i64>,
}

impl Features {
    fn new(p: &PhotoFacts<'_>, mesh: bool) -> Features {
        let s = &p.metrics.sharpness;
        let e = &p.metrics.exposure;
        let main = main_face(p.faces, mesh);
        // Eyes are only judged on faces big enough to read them: below
        // `EYES_MIN_AREA` (≈ 60 px wide in the analysed image) the landmarks
        // are guesses — on real shoots, children far away in a pool were
        // "eyes closed". A group photo of 8-10 people stays above it.
        let readable = |f: &FaceObservation| f.area_ratio >= EYES_MIN_AREA;
        let sharp = |f: &FaceObservation| clean(f.sharpness);
        let (face_area, face_focus, main_open) = match main {
            Some(f) => (f.area_ratio.min(1.0), sharp(f), if readable(f) { openness(f) } else { None }),
            None => (0.0, None, None),
        };
        let other_face_sharp = main.is_some_and(|m| {
            p.faces.iter().any(|f| {
                !std::ptr::eq(f, m)
                    && is_subject(f, mesh)
                    && f.area_ratio >= MAIN_FACE_MIN_AREA
                    && sharp(f).is_some_and(|v| v >= SUBJECT_SHARP_FACE)
            })
        });
        let head_away = main.and_then(|f| f.pose).is_some_and(|pose| {
            let away = |v: f32, limit: f32| v.is_finite() && v.abs() > limit;
            away(pose.yaw, HEAD_AWAY_YAW) || away(pose.pitch, HEAD_AWAY_PITCH) || away(pose.roll, HEAD_AWAY_ROLL)
        });
        let mut group_open = Vec::new();
        if let Some(m) = main {
            let comparable: Vec<f32> = p
                .faces
                .iter()
                .filter(|f| usable(f))
                .filter(|f| f.area_ratio >= COMPARABLE_FACE_RATIO * m.area_ratio && readable(f))
                .filter_map(openness)
                .collect();
            if comparable.len() >= GROUP_PHOTO_MIN_FACES {
                group_open = comparable;
            }
        }
        // Without a main face, the subject region (a figure seen from the
        // back, an animal) judges the focus and the overexposure.
        let subject =
            p.subject.filter(|s| main.is_none() && s.area_ratio.is_finite() && s.area_ratio >= SUBJECT_MIN_AREA);
        let subject_focus = subject.and_then(|s| clean(s.sharpness));
        let frame_exposure = unit(e.score, 0.0);
        let frame_bright = finite_or(e.mean_luma, 0.5) >= 0.5;
        // The skin is only trusted on a face the mesh is sure of (a camera
        // or a hand held in front of the face reads as dark skin).
        let sure = |f: &&FaceObservation| f.mesh_presence.is_none_or(|p| p >= SKIN_MESH_PRESENCE);
        let skin = main
            .filter(|f| readable(f))
            .filter(sure)
            .and_then(|f| Some((clean(f.skin_luma)?, clean(f.skin_clipped).unwrap_or(0.0))));
        let subject_over = subject.and_then(|s| s.ev_over).filter(|v| v.is_finite());
        let (exposure, bright) = match (skin, subject_over) {
            (Some((median, clipped)), _) => exposure::with_skin(frame_exposure, frame_bright, median, clipped),
            (None, Some(over)) => (exposure::with_subject(frame_exposure, frame_bright, over), frame_bright),
            (None, None) => (frame_exposure, frame_bright),
        };
        let frame_exposure = skin.map(|_| frame_exposure);
        Features {
            frame_focus: unit(s.score, 0.0),
            motion: unit(s.motion_blur, 0.0),
            face_area,
            face_focus,
            other_face_sharp,
            head_away,
            subject_focus,
            main_open,
            group_open,
            exposure,
            bright,
            frame_exposure,
            aesthetic: p.aesthetic.filter(|a| a.is_finite()),
            time: p.exif.captured_at_ms,
        }
    }

    /// The main face is big enough to trust its focus and is sharp (very
    /// sharp under frame-level motion of at least `motion_block`: pass
    /// infinity for the plain test, 0 for the very sharp one).
    fn subject_sharp(&self, motion_block: f32) -> bool {
        let needed = if self.motion >= motion_block { SUBJECT_SHARP_UNDER_MOTION } else { SUBJECT_SHARP_FACE };
        (self.face_area >= FACE_RELIABLE_AREA && self.face_focus.is_some_and(|f| f >= needed))
            || self.subject_focus.is_some_and(|f| f >= needed)
    }

    fn focus_sub(&self, settings: &CullSettings) -> f32 {
        let base = match self.face_focus.or(self.subject_focus) {
            Some(f) => FACE_FOCUS_WEIGHT * f + (1.0 - FACE_FOCUS_WEIGHT) * self.frame_focus,
            None => self.frame_focus,
        };
        // Sensitivity independent: the Balanced blocking level of motion.
        let sharp = self.subject_sharp(Params::new(Sensitivity::Balanced).motion_block);
        let motion = if sharp || !settings.detect_motion_blur { 0.0 } else { self.motion };
        base * (1.0 - MOTION_FOCUS_PENALTY * motion)
    }

    /// The exposure detection of this photo's direction is on.
    fn exposure_checked(&self, settings: &CullSettings) -> bool {
        if self.bright {
            settings.detect_overexposed
        } else {
            settings.detect_underexposed
        }
    }

    fn eyes_sub(&self) -> Option<f32> {
        if !self.group_open.is_empty() {
            let open = self.group_open.iter().filter(|&&o| o >= EYES_SCORE_CLOSED).count();
            let share = open as f32 / self.group_open.len() as f32;
            let main = self.main_open.unwrap_or(share);
            Some(0.5 * main + 0.5 * share)
        } else {
            self.main_open
        }
    }
}

// ---------------------------------------------------------------------------
// Shoot context
// ---------------------------------------------------------------------------

fn percentile(values: &mut [f32], q: f32) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let pos = q.clamp(0.0, 1.0) * (values.len() - 1) as f32;
    let (i, frac) = (pos.floor() as usize, pos.fract());
    let j = (i + 1).min(values.len() - 1);
    Some(values[i] + frac * (values[j] - values[i]))
}

fn reference(mut values: Vec<f32>, prior: f32) -> f32 {
    let n = values.len() as f32;
    match percentile(&mut values, REF_PERCENTILE) {
        Some(p) => (n * p + REF_PRIOR_WEIGHT * prior) / (n + REF_PRIOR_WEIGHT),
        None => prior,
    }
}

#[derive(Debug, Clone, Copy)]
struct ShootContext {
    focus_ref: f32,
    face_ref: f32,
    exposure_ref: f32,
    aes_median: f32,
    aes_spread: f32,
    aes_n: f32,
}

impl ShootContext {
    fn new(f: &[Features]) -> ShootContext {
        let focus_ref = reference(f.iter().map(|x| x.frame_focus).collect(), FOCUS_REF_PRIOR);
        let face_ref = reference(f.iter().filter_map(|x| x.face_focus).collect(), FACE_REF_PRIOR);
        let exposure_ref = reference(f.iter().map(|x| x.exposure).collect(), EXPOSURE_REF_PRIOR);
        let mut aes: Vec<f32> = f.iter().filter_map(|x| x.aesthetic).collect();
        let aes_n = aes.len() as f32;
        let aes_median = percentile(&mut aes, 0.5).unwrap_or(AES_ABS_CENTER);
        let mut dev: Vec<f32> = aes.iter().map(|a| (a - aes_median).abs()).collect();
        let mad = percentile(&mut dev, 0.5).unwrap_or(0.0);
        ShootContext {
            focus_ref,
            face_ref,
            exposure_ref,
            aes_median,
            aes_spread: (1.4826 * mad).max(AES_SPREAD_MIN),
            aes_n,
        }
    }

    /// Aesthetic z-score: relative to the shoot, blended with an absolute
    /// scale on small shoots.
    fn aesthetic_z(&self, a: f32) -> f32 {
        let z_rel = (a - self.aes_median) / self.aes_spread;
        let z_abs = (a - AES_ABS_CENTER) / AES_ABS_SPREAD;
        (self.aes_n * z_rel + AES_PRIOR_WEIGHT * z_abs) / (self.aes_n + AES_PRIOR_WEIGHT)
    }
}

fn logistic(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

// ---------------------------------------------------------------------------
// Scores
// ---------------------------------------------------------------------------

struct Scored {
    score: f32,
    breakdown: ScoreBreakdown,
}

fn score_one(f: &Features, ctx: &ShootContext, settings: &CullSettings, group_size: usize) -> Scored {
    let focus = f.focus_sub(settings);
    let eyes = f.eyes_sub();
    let aesthetic = f.aesthetic.map(|a| logistic(AES_SLOPE * ctx.aesthetic_z(a)));
    let uniqueness = 1.0 / (group_size.max(1) as f32).sqrt();
    let breakdown = ScoreBreakdown { focus, exposure: f.exposure, eyes, aesthetic, uniqueness };

    let mut parts: Vec<(f32, f32)> = Vec::with_capacity(5);
    let mut weakest = 1.0f32;
    if settings.checks_sharpness() {
        parts.push((W_FOCUS, focus));
        weakest = weakest.min(focus);
    }
    if f.exposure_checked(settings) {
        parts.push((W_EXPOSURE, f.exposure));
        weakest = weakest.min(f.exposure);
    }
    if let (true, Some(e)) = (settings.detect_eyes_closed, eyes) {
        parts.push((W_EYES, e));
    }
    parts.push((W_UNIQUENESS, uniqueness));
    if let Some(a) = aesthetic {
        // Exactly AESTHETIC_SHARE of the weighted mean.
        let others: f32 = parts.iter().map(|p| p.0).sum();
        parts.push((others * AESTHETIC_SHARE / (1.0 - AESTHETIC_SHARE), a));
    }
    let wsum: f32 = parts.iter().map(|p| p.0).sum();
    let mean = parts.iter().map(|p| p.0 * p.1).sum::<f32>() / wsum;
    let gate = if weakest < GATE_BELOW { GATE_FLOOR + (1.0 - GATE_FLOOR) / GATE_BELOW * weakest } else { 1.0 };
    let score = (100.0 * mean * gate).clamp(0.0, 100.0);
    Scored { score: if score.is_finite() { score } else { 0.0 }, breakdown }
}

/// Ranking scores used to order group members (uniqueness excluded: it is
/// the same for every member of a group).
pub(crate) fn ranking_scores(photos: &[PhotoFacts<'_>], settings: &CullSettings) -> Vec<f32> {
    let mesh = mesh_ran(photos);
    let feats: Vec<Features> = photos.iter().map(|p| Features::new(p, mesh)).collect();
    let ctx = ShootContext::new(&feats);
    feats.iter().map(|f| score_one(f, &ctx, settings, 1).score).collect()
}

/// Group ordering: score desc, then capture time asc (unknown last), then id.
pub(crate) fn rank_cmp(a: (f32, Option<i64>, &str), b: (f32, Option<i64>, &str)) -> std::cmp::Ordering {
    b.0.total_cmp(&a.0)
        .then_with(|| match (a.1, b.1) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        })
        .then_with(|| a.2.cmp(b.2))
}

// ---------------------------------------------------------------------------
// Defects
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
struct Defects {
    block: Vec<Reason>,
    soft: Vec<Reason>,
    obvious: bool,
    /// The main face's eyes read closed (whether or not this sensitivity
    /// flags it): explains a frame left out of its group.
    eyes_closed: bool,
}

impl Defects {
    fn add(&mut self, level: Level, reason: Reason) {
        match level {
            Level::None => {}
            Level::Soft => self.soft.push(reason),
            Level::Block => self.block.push(reason),
            Level::Obvious => {
                self.block.push(reason);
                self.obvious = true;
            }
        }
    }

    fn blocking(&self) -> bool {
        !self.block.is_empty()
    }
}

/// Blur defects as measured, each softness attributed to one cause, before
/// the settings drop the causes the photographer does not want flagged.
fn blur_defects(f: &Features, ctx: &ShootContext, p: &Params, artistic: bool) -> Vec<(Level, Reason)> {
    let motion = if f.motion >= p.motion_obvious {
        Level::Obvious
    } else if f.motion >= p.motion_block {
        Level::Block
    } else if f.motion >= p.motion_soft {
        Level::Soft
    } else {
        Level::None
    };
    let blur = p.focus.level(f.frame_focus, ctx.focus_ref);
    // Artistic blur: a soft frame may be a small sharp subject on bokeh, so
    // the frame only rejects when nothing in it is sharp (obvious level),
    // goes to review when clearly soft, and mild softness is not flagged.
    // The face measure decides nothing (see the module docs).
    let frame = |level: Level| match level {
        _ if !artistic => level,
        Level::Obvious => Level::Obvious,
        Level::Block => Level::Soft,
        Level::Soft | Level::None => Level::None,
    };
    let mut out = vec![(frame(motion), Reason::MotionBlur)];
    // Motion blur explains the softness better than "blurry" when both fire.
    if blur > motion {
        out.push((frame(blur), Reason::Blurry));
    }
    // The subject: a main face (confirmed by the mesh), looking at us, and
    // no other face sharp (the focus is on someone else, on purpose).
    if blur < Level::Block && !artistic && !f.other_face_sharp && !f.head_away {
        if let Some(ff) = f.face_focus {
            let mut level = p.face.level(ff, ctx.face_ref);
            if f.face_area < SUBJECT_BLOCK_AREA {
                level = level.min(Level::Soft);
            }
            out.push((level, Reason::SubjectOutOfFocus));
        }
    }
    out
}

fn defects(f: &Features, ctx: &ShootContext, p: &Params, settings: &CullSettings) -> Defects {
    let mut d = Defects::default();
    // A sharp main face: the frame-level measures are not about the subject.
    // A sharp face under strong frame motion: to review at most.
    let capped = f.subject_sharp(f32::INFINITY) && !f.subject_sharp(p.motion_block);
    if settings.checks_sharpness() && !f.subject_sharp(p.motion_block) {
        for (level, reason) in blur_defects(f, ctx, p, settings.artistic_blur) {
            let wanted = match reason {
                Reason::MotionBlur => settings.detect_motion_blur,
                Reason::Blurry => settings.detect_blur,
                _ => settings.detect_subject_focus,
            };
            if wanted {
                d.add(if capped { level.min(Level::Soft) } else { level }, reason);
            }
        }
    }
    if settings.detect_eyes_closed {
        let mut level = Level::None;
        if !f.group_open.is_empty() {
            let closed = f.group_open.iter().filter(|&&o| o < p.eyes_closed_below).count();
            let share = closed as f32 / f.group_open.len() as f32;
            if share >= p.eyes_group_block {
                level = Level::Block;
            } else if share >= p.eyes_group_soft {
                level = Level::Soft;
            }
        } else if let Some(o) = f.main_open {
            d.eyes_closed = o < p.eyes_closed_below;
            if p.eyes_on_portraits {
                if o < p.eyes_closed_below {
                    level = Level::Block;
                } else if o < p.eyes_closed_below + EYES_SOFT_MARGIN {
                    level = Level::Soft;
                }
            }
        }
        if !p.eyes_can_reject {
            level = level.min(Level::Soft);
        }
        d.add(level, Reason::EyesClosed);
    }
    if f.exposure_checked(settings) {
        let mut level = p.exposure.level(f.exposure, ctx.exposure_ref);
        // The skin alone (a face in the shade, behind sunglasses, a blown
        // face in a fine frame) sends to review; rejecting needs the frame
        // to agree.
        if let Some(frame) = f.frame_exposure {
            if level > Level::Soft && p.exposure.level(frame, ctx.exposure_ref) < Level::Block {
                level = Level::Soft;
            }
        }
        d.add(level, if f.bright { Reason::Overexposed } else { Reason::Underexposed });
    }
    d
}

// ---------------------------------------------------------------------------
// assess
// ---------------------------------------------------------------------------

struct Decision {
    verdict: Verdict,
    reasons: Vec<Reason>,
    decided: bool,
}

pub(crate) fn assess(photos: &[PhotoFacts<'_>], groups: &[SimilarGroup], settings: &CullSettings) -> Vec<Assessment> {
    let n = photos.len();
    if n == 0 {
        return Vec::new();
    }
    let params = Params::new(settings.sensitivity);
    let keep_per_group = settings.keep_per_group.clamp(1, 5) as usize;
    let mesh = mesh_ran(photos);
    let feats: Vec<Features> = photos.iter().map(|p| Features::new(p, mesh)).collect();
    let ctx = ShootContext::new(&feats);
    let index: HashMap<&str, usize> = {
        let mut m = HashMap::with_capacity(n);
        for (i, p) in photos.iter().enumerate() {
            m.entry(p.id).or_insert(i);
        }
        m
    };

    // Active groups: exact copies when their detection is on, bursts/similar
    // when grouping is on.
    let mut member_of: Vec<Option<usize>> = vec![None; n];
    let mut active: Vec<(u32, GroupKind, Vec<usize>)> = Vec::new();
    for g in groups {
        let wanted = if g.kind == GroupKind::Exact { settings.detect_exact_duplicates } else { settings.group_similar };
        if !wanted {
            continue;
        }
        let mut members = Vec::with_capacity(g.photo_ids.len());
        for id in &g.photo_ids {
            if let Some(&i) = index.get(id.as_str()) {
                if member_of[i].is_none() {
                    member_of[i] = Some(active.len());
                    members.push(i);
                }
            }
        }
        if members.len() >= 2 {
            active.push((g.id, g.kind, members));
        } else {
            for i in members {
                member_of[i] = None;
            }
        }
    }

    let scored: Vec<Scored> = feats
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let size = member_of[i].map(|g| active[g].2.len()).unwrap_or(1);
            score_one(f, &ctx, settings, size)
        })
        .collect();
    let defects: Vec<Defects> = feats.iter().map(|f| defects(f, &ctx, &params, settings)).collect();
    let key = |i: usize| (scored[i].score, feats[i].time, photos[i].id);

    let mut decisions: Vec<Decision> =
        (0..n).map(|_| Decision { verdict: Verdict::Keep, reasons: Vec::new(), decided: false }).collect();
    let mut group_rank = vec![0u32; n];

    // 1. Exact copies: same hashes and measurements, and inside `Exact` groups.
    let keys: Vec<CopyKey> = photos
        .iter()
        .map(|p| CopyKey {
            phash: p.metrics.hashes.phash,
            dhash: p.metrics.hashes.dhash,
            focus: p.metrics.sharpness.score,
            luma: p.metrics.exposure.mean_luma,
        })
        .collect();
    let mut copy_sets = if settings.detect_exact_duplicates { exact_copy_sets(&keys) } else { Vec::new() };
    for (_, kind, members) in &active {
        if *kind == GroupKind::Exact {
            copy_sets.push(members.clone());
        }
    }
    let mut is_copy = vec![false; n];
    for mut set in copy_sets {
        set.sort_by(|&a, &b| rank_cmp(key(a), key(b)));
        for &i in &set[1..] {
            is_copy[i] = true;
        }
    }
    for i in 0..n {
        if is_copy[i] {
            let d = &mut decisions[i];
            d.verdict = Verdict::Reject;
            d.reasons.push(Reason::ExactDuplicate);
            d.reasons.extend(defects[i].block.iter().copied());
            d.decided = true;
        }
    }

    // 2. Groups of similar shots.
    for (_, kind, members) in &active {
        let mut ranked = members.clone();
        ranked.sort_by(|&a, &b| rank_cmp(key(a), key(b)));
        for (r, &i) in ranked.iter().enumerate() {
            group_rank[i] = r as u32;
        }
        if *kind == GroupKind::Exact {
            continue; // copies handled above, the original is judged as a single
        }
        let candidates: Vec<usize> =
            ranked.iter().copied().filter(|&i| !is_copy[i] && !defects[i].blocking()).collect();
        for &i in &ranked {
            if !is_copy[i] && defects[i].blocking() {
                let d = &mut decisions[i];
                d.verdict = Verdict::Reject;
                d.reasons.extend(defects[i].block.iter().copied());
                d.decided = true;
            }
        }
        if candidates.is_empty() {
            // Never drop a whole moment silently.
            if let Some(&best) = ranked.iter().find(|&&i| !is_copy[i] && !defects[i].obvious) {
                decisions[best].verdict = Verdict::Maybe;
            }
            continue;
        }
        // A burst is one moment: keep the setting's count, the rest are
        // duplicates. A series of similar frames (poses, expressions) keeps
        // one in five at least and never rejects the others.
        let burst = *kind == GroupKind::Burst;
        let keep = if burst { keep_per_group } else { keep_per_group.max(members.len().div_ceil(SERIES_KEEP_EVERY)) };
        let kept = &candidates[..keep.min(candidates.len())];
        let last_kept = scored[*kept.last().unwrap_or(&candidates[0])].score;
        let kept_eyes_open = kept.iter().any(|&i| !defects[i].eyes_closed);
        for (k, &i) in candidates.iter().enumerate() {
            let d = &mut decisions[i];
            d.decided = true;
            d.reasons.extend(defects[i].soft.iter().copied());
            if k < kept.len() {
                d.verdict = if defects[i].soft.is_empty() { Verdict::Keep } else { Verdict::Maybe };
            } else {
                d.reasons.push(Reason::Duplicate);
                if settings.detect_eyes_closed && defects[i].eyes_closed && kept_eyes_open {
                    // Why it lost: its eyes are closed, a kept one's are open.
                    d.reasons.push(Reason::EyesClosed);
                }
                let close = scored[i].score >= last_kept - params.group_margin;
                d.verdict = if !burst || close { Verdict::Maybe } else { Verdict::Reject };
            }
        }
    }

    // 3. Singles (and originals of exact copies).
    for i in 0..n {
        let d = &mut decisions[i];
        if d.decided {
            continue;
        }
        let def = &defects[i];
        if def.blocking() {
            d.verdict = Verdict::Reject;
            d.reasons.extend(def.block.iter().copied());
            d.reasons.extend(def.soft.iter().copied());
            continue;
        }
        d.reasons.extend(def.soft.iter().copied());
        d.verdict = if def.soft.is_empty() { Verdict::Keep } else { Verdict::Maybe };
    }

    (0..n)
        .map(|i| {
            let mut reasons = std::mem::take(&mut decisions[i].reasons);
            reasons.sort();
            reasons.dedup();
            Assessment {
                score: (scored[i].score * 10.0).round() / 10.0,
                verdict: decisions[i].verdict,
                reasons,
                group_id: member_of[i].map(|g| active[g].0),
                group_rank: group_rank[i],
                breakdown: scored[i].breakdown,
            }
        })
        .collect()
}
