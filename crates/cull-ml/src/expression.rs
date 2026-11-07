//! Face expression: MediaPipe Face Blendshapes V2 (Google, Apache-2.0,
//! converted to ONNX by Pelli, see `models/manifest.json`).
//!
//! Input: x, y of 146 Face Mesh landmarks (`SUBSET`, MediaPipe's
//! `kLandmarksSubsetIdxs`), in any isotropic pixel frame (the model centres
//! and scales them itself): the mesh output in pixels of its 256x256 crop.
//! Output: 52 ARKit blendshape scores in `[0, 1]`. "Left" is the subject's
//! left eye (mesh points 362-263, `eyes::LEFT_EYE`).
//!
//! The eye state combines both readings of the mesh (see
//! [`open_probability`]): the eye aspect ratio is decisive for eyes shut
//! tight, relaxed or squeezed (it reads 0.00-0.05, where the blendshapes
//! still give a blink of 0.4-0.55 with a high squint), and reads the eyes
//! narrowed by a smile or lowered over a downcast look as closed; the
//! blendshapes tell the smile apart (blink 0.3-0.5 with squint × smile).
//!
//! Always on the CPU (0.4-1.5 ms per face): CoreML splits the graph into 9
//! partitions and is slower.

use crate::runtime::{output_f32, SessionPool};
use crate::{MlError, Result};
use cull_types::FaceExpression;
use ort::value::TensorRef;

/// Mesh landmark indices fed to the model, in order.
pub(crate) const SUBSET: [usize; 146] = [
    0, 1, 4, 5, 6, 7, 8, 10, 13, 14, 17, 21, 33, 37, 39, 40, 46, 52, 53, 54, 55, 58, 61, 63, 65, 66, 67, 70, 78, 80,
    81, 82, 84, 87, 88, 91, 93, 95, 103, 105, 107, 109, 127, 132, 133, 136, 144, 145, 146, 148, 149, 150, 152, 153,
    154, 155, 157, 158, 159, 160, 161, 162, 163, 168, 172, 173, 176, 178, 181, 185, 191, 195, 197, 234, 246, 249, 251,
    263, 267, 269, 270, 276, 282, 283, 284, 285, 288, 291, 293, 295, 296, 297, 300, 308, 310, 311, 312, 314, 317, 318,
    321, 323, 324, 332, 334, 336, 338, 356, 361, 362, 365, 373, 374, 375, 377, 378, 379, 380, 381, 382, 384, 385, 386,
    387, 388, 389, 390, 397, 398, 400, 402, 405, 409, 415, 454, 466, 468, 469, 470, 471, 472, 473, 474, 475, 476, 477,
];
pub(crate) const OUTPUTS: usize = 52;
const EYE_BLINK_LEFT: usize = 9;
const EYE_BLINK_RIGHT: usize = 10;
const EYE_SQUINT_LEFT: usize = 19;
const EYE_SQUINT_RIGHT: usize = 20;
const JAW_OPEN: usize = 25;
const MOUTH_SMILE_LEFT: usize = 44;
const MOUTH_SMILE_RIGHT: usize = 45;

/// Revision of the reading of the scores (eye calibration), part of
/// `models_id`.
pub(crate) const ALGO_TAG: &str = "eye1";

/// Eye-open calibration (fitted on 26 hand-labelled faces of three real
/// shoots, checked on 220 sure-open faces: see the crate README):
/// `logit = EAR_WEIGHT × clamp(EAR log-odds, ±EAR_CAP) + (BLINK_CENTER − b) /
/// BLINK_SCALE`, with the blink corrected for the lids: a smile narrows the
/// eyes (`b −= LAUGH_DISCOUNT × squint × smile`), a squint without a smile
/// is lids closing (`b += SQUINT_PENALTY × squint × (1 − smile)`).
pub(crate) const BLINK_CENTER: f32 = 0.70;
pub(crate) const BLINK_SCALE: f32 = 0.05;
pub(crate) const LAUGH_DISCOUNT: f32 = 0.35;
pub(crate) const SQUINT_PENALTY: f32 = 0.2;
pub(crate) const EAR_WEIGHT: f32 = 0.5;
pub(crate) const EAR_CAP: f32 = 5.0;

pub(crate) struct ExpressionModel {
    pub pool: SessionPool,
}

/// The 146 model input points (x, y) of a 478-landmark mesh (x, y, z per
/// point); `None` when the mesh is too short or not finite.
pub(crate) fn input_points(landmarks: &[f32]) -> Option<[f32; 2 * SUBSET.len()]> {
    let mut out = [0f32; 2 * SUBSET.len()];
    for (k, &i) in SUBSET.iter().enumerate() {
        let (x, y) = (*landmarks.get(3 * i)?, *landmarks.get(3 * i + 1)?);
        if !(x.is_finite() && y.is_finite()) {
            return None;
        }
        out[2 * k] = x;
        out[2 * k + 1] = y;
    }
    Some(out)
}

/// The scores Pelli Instant keeps.
pub(crate) fn expression_of(scores: &[f32]) -> Option<FaceExpression> {
    if scores.len() < OUTPUTS || scores.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let s = |i: usize| Some(scores[i].clamp(0.0, 1.0));
    Some(FaceExpression {
        eye_blink_right: s(EYE_BLINK_RIGHT),
        eye_blink_left: s(EYE_BLINK_LEFT),
        eye_squint_right: s(EYE_SQUINT_RIGHT),
        eye_squint_left: s(EYE_SQUINT_LEFT),
        smile: Some(0.5 * (scores[MOUTH_SMILE_LEFT] + scores[MOUTH_SMILE_RIGHT]).clamp(0.0, 2.0)),
        jaw_open: s(JAW_OPEN),
    })
}

/// Probability that each eye (right, left) is open, from the blendshapes
/// and the eye aspect ratio log-odds of the same eye (`eyes::open_logit`;
/// `None` for an eye the mesh found hidden: it stays unknown).
pub(crate) fn open_probability(x: &FaceExpression, ear_logit: [Option<f32>; 2]) -> [Option<f32>; 2] {
    let smile = x.smile.unwrap_or(0.0).clamp(0.0, 1.0);
    let p = |blink: Option<f32>, squint: Option<f32>, ear: Option<f32>| {
        let sq = squint.unwrap_or(0.0);
        let b = blink? - LAUGH_DISCOUNT * sq * smile + SQUINT_PENALTY * sq * (1.0 - smile);
        let logit = EAR_WEIGHT * ear?.clamp(-EAR_CAP, EAR_CAP) + (BLINK_CENTER - b) / BLINK_SCALE;
        logit.is_finite().then(|| 1.0 / (1.0 + (-logit).exp()))
    };
    [p(x.eye_blink_right, x.eye_squint_right, ear_logit[0]), p(x.eye_blink_left, x.eye_squint_left, ear_logit[1])]
}

impl ExpressionModel {
    /// Blendshape scores of each mesh (478 x 3 landmarks in crop pixels).
    pub(crate) fn run(&self, meshes: &[&[f32]]) -> Result<Vec<Option<[f32; OUTPUTS]>>> {
        let points: Vec<Option<[f32; 2 * SUBSET.len()]>> = meshes.iter().map(|l| input_points(l)).collect();
        self.run_points(&points)
    }

    /// Blendshape scores of each set of 146 input points (`None` skipped).
    fn run_points(&self, all: &[Option<[f32; 2 * SUBSET.len()]>]) -> Result<Vec<Option<[f32; OUTPUTS]>>> {
        if all.iter().all(Option::is_none) {
            return Ok(vec![None; all.len()]);
        }
        self.pool.with(|session| {
            let mut out = Vec::with_capacity(all.len());
            for points in all {
                let Some(points) = points else {
                    out.push(None);
                    continue;
                };
                let tensor = TensorRef::from_array_view(([1usize, SUBSET.len(), 2], &points[..]))
                    .map_err(|e| MlError::Runtime(format!("blendshapes input: {e}")))?;
                let outputs =
                    session.run(ort::inputs![tensor]).map_err(|e| MlError::Runtime(format!("blendshapes: {e}")))?;
                let v = output_f32(&outputs, "blendshapes", OUTPUTS)?;
                let mut scores = [0f32; OUTPUTS];
                scores.copy_from_slice(&v[..OUTPUTS]);
                out.push(Some(scores));
            }
            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{build_first, ModelOpts, ModelSource};

    /// The converted model against MediaPipe's TFLite outputs (vectors from
    /// the conversion: two real faces, ORT 1.31 CPU, max |diff| 2e-6).
    #[test]
    fn matches_the_reference_vectors() {
        let dir = std::env::var_os("PELLI_CULL_MODELS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models"));
        let Ok(Some(m)) = crate::manifest::load_verified(&dir, "face-blendshapes") else {
            eprintln!("SKIP: face_blendshapes_v2.onnx absent");
            return;
        };
        let (first, spec) =
            build_first("face-blendshapes", ModelSource::Memory(m.bytes.into()), None, &ModelOpts::cpu(1)).unwrap();
        let model = ExpressionModel { pool: SessionPool::new(first, spec, None, 1) };
        let v: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/face_blendshapes_v2.testvec.json")).unwrap();
        let subset: Vec<usize> = serde_json::from_value(v["subset"].clone()).unwrap();
        assert_eq!(subset, SUBSET);
        for case in v["cases"].as_array().unwrap() {
            let pts: Vec<[f32; 2]> = serde_json::from_value(case["points"].clone()).unwrap();
            let expected: Vec<f32> = serde_json::from_value(case["expected"].clone()).unwrap();
            let mut input = [0f32; 2 * SUBSET.len()];
            for (k, p) in pts.iter().enumerate() {
                input[2 * k] = p[0];
                input[2 * k + 1] = p[1];
            }
            let got = model.run_points(&[Some(input), None]).unwrap();
            assert!(got[1].is_none());
            let got = got[0].unwrap();
            let diff = got.iter().zip(&expected).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
            assert!(diff < 1e-4, "{}: max diff {diff}", case["photo"]);
        }
    }

    #[test]
    fn subset_is_sorted_and_in_range() {
        assert!(SUBSET.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(*SUBSET.last().unwrap(), 477);
        let mut lms = vec![0f32; 3 * 478];
        lms[3 * 477] = 7.0;
        lms[3 * 477 + 1] = 9.0;
        let p = input_points(&lms).unwrap();
        assert_eq!((p[2 * 145], p[2 * 145 + 1]), (7.0, 9.0));
        assert!(input_points(&lms[..30]).is_none());
        lms[0] = f32::NAN;
        assert!(input_points(&lms).is_none());
    }

    #[test]
    fn laughs_read_open_blinks_closed() {
        let x = |blink: f32, squint: f32, smile: f32| FaceExpression {
            eye_blink_right: Some(blink),
            eye_blink_left: Some(blink),
            eye_squint_right: Some(squint),
            eye_squint_left: Some(squint),
            smile: Some(smile),
            jaw_open: Some(0.0),
        };
        let ear = |e: f32| [Some(crate::eyes::open_logit(e)); 2];
        let p = |x: FaceExpression, e: f32| open_probability(&x, ear(e))[0].unwrap();
        // Values measured on real shoots (crate README). An open smile: EAR
        // 0.15 (closed for the EAR alone), blink 0.34, squint 0.4, smile 0.75.
        assert!(p(x(0.34, 0.4, 0.75), 0.15) > 0.95);
        // A blink: EAR 0.09, blink 0.67.
        assert!(p(x(0.67, 0.22, 0.13), 0.09) < 0.15);
        // Eyes shut and relaxed: EAR 0.06, blink 0.5 but squint 0.8
        // (closed on Balanced, below 0.35).
        assert!(p(x(0.49, 0.8, 0.0), 0.06) < 0.3);
        // Open, looking down: EAR 0.12, blink 0.5, squint 0.3.
        assert!(p(x(0.50, 0.3, 0.1), 0.12) > 0.6);
        // Wide open whatever the EAR noise.
        assert!(p(x(0.2, 0.1, 0.0), 0.10) > 0.99);
        // A hidden eye stays unknown.
        assert_eq!(open_probability(&x(0.9, 0.0, 0.0), [None, Some(0.0)])[0], None);
        assert_eq!(open_probability(&FaceExpression::default(), ear(0.2)), [None, None]);
        assert!(expression_of(&[0.5; 10]).is_none());
        let e = expression_of(&[0.5; OUTPUTS]).unwrap();
        assert_eq!(e.smile, Some(0.5));
    }
}
