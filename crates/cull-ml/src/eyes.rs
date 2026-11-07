//! Face mesh reading: eye state (open / closed), head pose and mouth opening
//! from MediaPipe Face Mesh V2 landmarks (Apache-2.0, ONNX conversion by
//! PINTO_model_zoo).
//!
//! For each face, a square crop of `1.3 x max(face w, h)` centred on the
//! YuNet box and rotated so that the eye line is horizontal is sampled into
//! 256x256 RGB `[0, 1]`, from the most detailed image available: the
//! full-resolution detail image when the engine provided one (small faces,
//! groups), else the analysed preview, or its 1280 px working copy when that
//! is already large enough. The mesh returns 478 landmarks and a
//! face-presence logit.
//!
//! **Tilted heads**: YuNet's eye points are unreliable on a strongly tilted
//! head (−2° read for −52°): the crop is not straightened and the mesh puts
//! the eyes on the eyebrows. When the mesh's own eye line (outer corners 33 →
//! 263) differs from the crop's by more than `RECROP_DEGREES`, the face is
//! read again on MediaPipe's tracking crop: straightened along that line,
//! centred on the landmarks, `MESH_CROP_SCALE` x their extent (at most
//! `MAX_PASSES` passes). Faces within 8° keep the first crop, on which the
//! eye calibration was made.
//!
//! From the landmarks:
//!
//! * **eye state**: the eye aspect ratio
//!   `EAR = (|p2-p6| + |p3-p5|) / (2 |p1-p4|)` of each eye, mapped to an
//!   "open" probability by a logistic curve centred on `EAR_CENTER` (0.145,
//!   width 0.012): eyes really closed read 0.04-0.13, open eyes narrowed by a
//!   smile or a laugh 0.155-0.19, open eyes ≥ 0.2. When the blendshapes model
//!   ran, the probability combines the EAR with blink, squint and smile (see
//!   the `expression` module): a smile is then told apart from a blink. A far
//!   eye seen at a steep angle is `None`;
//! * **pose**: yaw from the depth difference across the face contour
//!   (points 234 → 454), pitch from forehead → chin (10 → 152), roll from the
//!   eye corners (33 → 263) plus the crop's own rotation; mouth opening =
//!   inner-lip gap (13-14) / inner mouth width (78-308);
//! * **presence**: the mesh's own face probability. YuNet sometimes boxes a
//!   bicycle part or a hand with confidence 0.6-0.75; the mesh does not
//!   recognise it as a face.
//!
//! Faces smaller than ~48 px in the sampled image (a frontal face with
//! [`MIN_IOD_PX`] between the eyes), and faces the mesh does not recognise
//! (presence < 0.5: strong profile, occlusion, not a face), get no eye
//! state.

use crate::faces::Face;
use crate::imgproc::{sample_rotated_chw, RgbView};
use crate::runtime::{output_f32, SessionPool};
use crate::{MlError, Result};
use cull_types::{FaceExpression, HeadPose};
use ort::value::TensorRef;

/// Input side of the mesh model.
const INPUT: usize = 256;
/// Crop side relative to the YuNet box (MediaPipe uses 1.5 on BlazeFace
/// boxes; YuNet's are larger, 1.3 gave the best separation on our photos).
const CROP_SCALE: f32 = 1.3;
/// Below this inter-ocular distance (pixels of the sampled image) a frontal
/// face is too small for the mesh: ~ a face 45 px wide.
pub(crate) const MIN_IOD_PX: f32 = 20.0;
/// The mesh runs on faces whose box is at least this large (pixels of the
/// sampled image): a frontal face with `MIN_IOD_PX` between the eyes, and
/// profiles of the same size (their eyes are close together in the image).
const MIN_FACE_PX: f32 = 2.4 * MIN_IOD_PX;
/// The mesh runs on at most this many faces (largest first).
pub(crate) const MAX_FACES: usize = 24;
/// Faces per inference batch.
const BATCH: usize = 8;
/// Logistic calibration of the EAR: p_open = sigmoid((ear - center) / scale).
const EAR_CENTER: f32 = 0.145;
const EAR_SCALE: f32 = 0.012;
pub(crate) const MIN_PRESENCE: f32 = 0.5;
/// An eye narrower than this fraction of the other one is considered hidden.
const HIDDEN_EYE_RATIO: f32 = 0.45;

/// Subject's right eye (left in the image): outer corner, two upper-lid
/// points, inner corner, two lower-lid points (MediaPipe mesh indices).
const RIGHT_EYE: [usize; 6] = [33, 160, 158, 133, 153, 144];
const LEFT_EYE: [usize; 6] = [362, 385, 387, 263, 373, 380];
/// Face contour at the cheekbones (subject's right, left), forehead top,
/// chin, inner lips (upper, lower) and inner mouth corners.
const CHEEK_R: usize = 234;
const CHEEK_L: usize = 454;
const FOREHEAD: usize = 10;
const CHIN: usize = 152;
const LIP_UP: usize = 13;
const LIP_DOWN: usize = 14;
const MOUTH_R: usize = 78;
const MOUTH_L: usize = 308;
const LANDMARKS: usize = 478;

/// Revision of the mesh reading (EAR calibration, pose, re-crop of tilted
/// heads), part of `models_id`.
pub(crate) const ALGO_TAG: &str = "ear3";

pub(crate) struct EyeModel {
    pub pool: SessionPool,
}

/// What the mesh says about one face.
#[derive(Debug, Clone, Default)]
pub(crate) struct MeshReading {
    /// Face-presence probability.
    pub presence: f32,
    pub right_open: Option<f32>,
    pub left_open: Option<f32>,
    /// Eye aspect ratios (`None` for a hidden eye), for the expression model.
    pub right_ear: Option<f32>,
    pub left_ear: Option<f32>,
    pub pose: Option<HeadPose>,
    /// Blendshape scores, filled after the mesh by the expression model.
    pub expression: Option<FaceExpression>,
    /// The 478 landmarks (x, y, z in crop pixels), for the expression model.
    pub landmarks: Vec<f32>,
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn pt(l: &[f32], a: usize) -> [f32; 3] {
    [l[3 * a], l[3 * a + 1], l[3 * a + 2]]
}

fn dist(l: &[f32], a: usize, b: usize) -> f32 {
    let (p, q) = (pt(l, a), pt(l, b));
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
}

/// Eye aspect ratio and eye width (crop pixels).
fn ear(l: &[f32], idx: &[usize; 6]) -> (f32, f32) {
    let width = dist(l, idx[0], idx[3]);
    let e = (dist(l, idx[1], idx[5]) + dist(l, idx[2], idx[4])) / (2.0 * width.max(1e-3));
    (e, width)
}

/// Log-odds that an eye with this EAR is open.
pub(crate) fn open_logit(ear: f32) -> f32 {
    if !ear.is_finite() {
        return 0.0;
    }
    (ear - EAR_CENTER) / EAR_SCALE
}

/// Probability that an eye with this EAR is open.
pub(crate) fn open_probability(ear: f32) -> f32 {
    sigmoid(open_logit(ear))
}

/// Head pose (degrees) from the landmarks of a crop rotated by `crop_angle`
/// radians.
fn head_pose(l: &[f32], crop_angle: f32) -> HeadPose {
    let (r, lft) = (pt(l, CHEEK_R), pt(l, CHEEK_L));
    let yaw = (lft[2] - r[2]).atan2((lft[0] - r[0]).max(1e-3)).to_degrees();
    let (top, chin) = (pt(l, FOREHEAD), pt(l, CHIN));
    let pitch = (chin[2] - top[2]).atan2((chin[1] - top[1]).max(1e-3)).to_degrees();
    let (er, el) = (pt(l, RIGHT_EYE[0]), pt(l, LEFT_EYE[3]));
    let roll = (el[1] - er[1]).atan2(el[0] - er[0]) + crop_angle;
    // Wrap to (-180, 180].
    let mut roll = roll.to_degrees();
    if roll > 180.0 {
        roll -= 360.0;
    } else if roll <= -180.0 {
        roll += 360.0;
    }
    let mouth_open = dist(l, LIP_UP, LIP_DOWN) / dist(l, MOUTH_R, MOUTH_L).max(1e-3);
    HeadPose { yaw, pitch, roll, mouth_open: mouth_open.clamp(0.0, 2.0) }
}

/// Reading of one face's mesh output.
fn read_mesh(landmarks: &[f32], presence_logit: f32, crop_angle: f32) -> MeshReading {
    let presence = if presence_logit.is_finite() { sigmoid(presence_logit) } else { 0.0 };
    if presence < MIN_PRESENCE || landmarks.len() < 3 * LANDMARKS || landmarks.iter().any(|v| !v.is_finite()) {
        return MeshReading { presence, ..Default::default() };
    }
    let (ear_r, w_r) = ear(landmarks, &RIGHT_EYE);
    let (ear_l, w_l) = ear(landmarks, &LEFT_EYE);
    // A far eye seen at a steep angle (turned head) is foreshortened, often
    // hidden by the nose, and its EAR is meaningless. (YuNet's nose position
    // proved a worse cue than the mesh's own eye widths.)
    let right_visible = w_r >= HIDDEN_EYE_RATIO * w_l;
    let left_visible = w_l >= HIDDEN_EYE_RATIO * w_r;
    MeshReading {
        presence,
        right_open: right_visible.then(|| open_probability(ear_r)),
        left_open: left_visible.then(|| open_probability(ear_l)),
        right_ear: right_visible.then_some(ear_r),
        left_ear: left_visible.then_some(ear_l),
        pose: Some(head_pose(landmarks, crop_angle)),
        expression: None,
        landmarks: landmarks[..3 * LANDMARKS].to_vec(),
    }
}

/// One face crop: centre and side in pixels of the image the faces were
/// detected on, rotation (radians, crop x axis along `(cos, sin)`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Crop {
    pub cx: f32,
    pub cy: f32,
    pub side: f32,
    pub angle: f32,
}

impl Crop {
    /// The first crop of a face: `CROP_SCALE` x its box, eye line level.
    fn of_face(f: &Face) -> Crop {
        let [r, l] = [f.kps[0], f.kps[1]];
        Crop {
            cx: f.x + f.w / 2.0,
            cy: f.y + f.h / 2.0,
            side: CROP_SCALE * f.w.max(f.h),
            angle: (l[1] - r[1]).atan2(l[0] - r[0]),
        }
    }

    /// Image coordinates of a point in crop pixels (`INPUT` x `INPUT`).
    fn image_point(&self, x: f32, y: f32) -> [f32; 2] {
        let step = self.side / INPUT as f32;
        let (u, v) = (x * step - self.side * 0.5, y * step - self.side * 0.5);
        let (sa, ca) = self.angle.sin_cos();
        [self.cx + u * ca - v * sa, self.cy + u * sa + v * ca]
    }

    /// MediaPipe's tracking crop from the mesh of this crop: rotated so the
    /// outer eye corners (33 → 263) are level, centred on the landmarks' box
    /// in that frame, `MESH_CROP_SCALE` x its long side. `None` when the
    /// eye line already is within `RECROP_DEGREES` of this crop's.
    fn refined(&self, landmarks: &[f32]) -> Option<Crop> {
        let n = landmarks.len() / 3;
        if n < LANDMARKS {
            return None;
        }
        let (a, b) = (pt(landmarks, RIGHT_EYE[0]), pt(landmarks, LEFT_EYE[3]));
        let delta = (b[1] - a[1]).atan2(b[0] - a[0]);
        if !delta.is_finite() || delta.abs().to_degrees() <= RECROP_DEGREES {
            return None;
        }
        let angle = self.angle + delta;
        let (sa, ca) = angle.sin_cos();
        let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
        for i in 0..LANDMARKS {
            let [x, y] = self.image_point(landmarks[3 * i], landmarks[3 * i + 1]);
            // Rotate by -angle: the eye line becomes horizontal.
            let r = [x * ca + y * sa, -x * sa + y * ca];
            for k in 0..2 {
                lo[k] = lo[k].min(r[k]);
                hi[k] = hi[k].max(r[k]);
            }
        }
        let (mx, my) = (0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1]));
        let side = MESH_CROP_SCALE * (hi[0] - lo[0]).max(hi[1] - lo[1]);
        let crop = Crop { cx: mx * ca - my * sa, cy: mx * sa + my * ca, side, angle };
        ([crop.cx, crop.cy, crop.side, crop.angle].iter().all(|v| v.is_finite()) && side > 1.0).then_some(crop)
    }
}

/// Re-crop when the mesh's eye line differs from the crop's by more than
/// this (degrees): YuNet's eye points are wrong on strongly tilted heads
/// (−2° read for −52°), the crop was not straightened and the mesh put the
/// eyes on the eyebrows. Below, the first crop is kept (the eye calibration
/// was made on it).
const RECROP_DEGREES: f32 = 8.0;
/// Mesh passes per face at most (the first crop + 2 re-crops).
const MAX_PASSES: usize = 3;
/// Side of a re-crop relative to the landmarks' box (MediaPipe: 1.5).
const MESH_CROP_SCALE: f32 = 1.5;

impl EyeModel {
    /// Mesh reading for each face (same order; `None` when not run). Face
    /// coordinates are in pixels of the image the faces were detected on;
    /// `detail` is that image (or a larger version of it, scaled by
    /// `detail_scale`), `work` a copy with its long side ≤ 1280 px. A face
    /// whose mesh finds the eye line tilted from the crop's is read again on
    /// a straightened crop (at most `MAX_PASSES` passes).
    pub(crate) fn read(
        &self,
        detail: RgbView<'_>,
        detail_scale: f32,
        work: RgbView<'_>,
        work_scale: f32,
        faces: &[Face],
    ) -> Result<Vec<Option<MeshReading>>> {
        let mut out: Vec<Option<MeshReading>> = vec![None; faces.len()];
        // Faces are sorted by decreasing area: the first MAX_FACES big enough.
        let mut todo: Vec<(usize, Crop)> = (0..faces.len())
            .filter(|&i| faces[i].w.max(faces[i].h) * detail_scale >= MIN_FACE_PX)
            .take(MAX_FACES)
            .map(|i| (i, Crop::of_face(&faces[i])))
            .collect();
        for _pass in 0..MAX_PASSES {
            if todo.is_empty() {
                break;
            }
            let results = self.run(detail, detail_scale, work, work_scale, &todo)?;
            let mut again = Vec::new();
            for ((i, crop), (reading, landmarks)) in todo.iter().zip(results) {
                let previous_ok = out[*i].as_ref().is_some_and(|r| r.presence >= MIN_PRESENCE);
                if reading.presence < MIN_PRESENCE && previous_ok {
                    continue; // the straightened crop lost the face: keep the first reading
                }
                if reading.presence >= MIN_PRESENCE {
                    if let Some(next) = crop.refined(&landmarks) {
                        log::debug!(
                            "cull-ml: face {i} re-cropped: eye line {:.0}° instead of {:.0}°",
                            next.angle.to_degrees(),
                            crop.angle.to_degrees()
                        );
                        again.push((*i, next));
                    }
                }
                out[*i] = Some(reading);
            }
            todo = again;
        }
        Ok(out)
    }

    /// One mesh pass on `crops`, batched: each face's reading and its raw
    /// landmarks.
    fn run(
        &self,
        detail: RgbView<'_>,
        detail_scale: f32,
        work: RgbView<'_>,
        work_scale: f32,
        crops: &[(usize, Crop)],
    ) -> Result<Vec<(MeshReading, Vec<f32>)>> {
        let plane = 3 * INPUT * INPUT;
        let mut all = Vec::with_capacity(crops.len());
        for chunk in crops.chunks(BATCH) {
            let mut input = vec![0f32; chunk.len() * plane];
            for (slot, (_, c)) in chunk.iter().enumerate() {
                let dst = &mut input[slot * plane..(slot + 1) * plane];
                if c.side * work_scale >= INPUT as f32 {
                    // The working copy is detailed enough and much cheaper to sample.
                    let s = work_scale;
                    sample_rotated_chw(work, c.cx * s, c.cy * s, c.side * s, c.angle, INPUT, dst);
                } else {
                    let s = detail_scale;
                    sample_rotated_chw(detail, c.cx * s, c.cy * s, c.side * s, c.angle, INPUT, dst);
                }
            }
            let n = chunk.len();
            let readings = self.pool.with(|session| {
                let tensor = TensorRef::from_array_view(([n, 3, INPUT, INPUT], &input[..]))
                    .map_err(|e| MlError::Runtime(format!("face mesh input: {e}")))?;
                let outputs =
                    session.run(ort::inputs![tensor]).map_err(|e| MlError::Runtime(format!("face mesh: {e}")))?;
                let lms = output_f32(&outputs, "Identity", n * 3 * LANDMARKS)?;
                let presence = output_f32(&outputs, "Identity_1", n)?;
                Ok((0..n)
                    .map(|slot| {
                        let l = &lms[slot * 3 * LANDMARKS..(slot + 1) * 3 * LANDMARKS];
                        (read_mesh(l, presence[slot], chunk[slot].1.angle), l.to_vec())
                    })
                    .collect::<Vec<_>>())
            })?;
            all.extend(readings);
        }
        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibration_is_monotonic_and_centered() {
        assert!(open_probability(0.05) < 0.01);
        assert!((open_probability(EAR_CENTER) - 0.5).abs() < 1e-6);
        assert!(open_probability(0.22) > 0.99);
        // Eyes narrowed by a smile (EAR 0.16-0.19) read open.
        assert!(open_probability(0.16) > 0.75, "{}", open_probability(0.16));
        // Eyes closed by a blink or a laugh (≤ 0.13) read closed.
        assert!(open_probability(0.13) < 0.25);
        assert_eq!(open_probability(f32::NAN), 0.5);
    }

    #[test]
    fn garbage_landmarks_give_nothing() {
        let r = read_mesh(&[f32::NAN; 3 * LANDMARKS], 10.0, 0.0);
        assert!(r.right_open.is_none() && r.left_open.is_none() && r.pose.is_none());
        assert!(read_mesh(&[0.0; 10], 10.0, 0.0).pose.is_none());
        // Face not recognised by the mesh: presence kept, nothing else.
        let r = read_mesh(&[1.0; 3 * LANDMARKS], -10.0, 0.0);
        assert!(r.presence < 0.01 && r.right_open.is_none() && r.pose.is_none());
    }

    #[test]
    fn pose_of_a_frontal_face() {
        // Flat, symmetric landmarks: no yaw, no pitch, roll = crop angle.
        let mut l = vec![0f32; 3 * LANDMARKS];
        let mut set = |i: usize, x: f32, y: f32| {
            l[3 * i] = x;
            l[3 * i + 1] = y;
        };
        set(CHEEK_R, 40.0, 120.0);
        set(CHEEK_L, 216.0, 120.0);
        set(FOREHEAD, 128.0, 30.0);
        set(CHIN, 128.0, 230.0);
        set(RIGHT_EYE[0], 70.0, 110.0);
        set(LEFT_EYE[3], 186.0, 110.0);
        set(LIP_UP, 128.0, 180.0);
        set(LIP_DOWN, 128.0, 200.0);
        set(MOUTH_R, 100.0, 190.0);
        set(MOUTH_L, 156.0, 190.0);
        let p = head_pose(&l, 0.1);
        assert!(p.yaw.abs() < 1e-3 && p.pitch.abs() < 1e-3);
        assert!((p.roll - 0.1f32.to_degrees()).abs() < 1e-3);
        assert!((p.mouth_open - 20.0 / 56.0).abs() < 1e-4);
    }

    #[test]
    fn a_tilted_mesh_straightens_the_crop() {
        // Landmarks of a face whose eye line is tilted by 30° in a level
        // crop of 256 px centred on (500, 400), 200 px wide in the image.
        let crop = Crop { cx: 500.0, cy: 400.0, side: 200.0, angle: 0.0 };
        let tilt = 30f32.to_radians();
        let mut l = vec![0f32; 3 * LANDMARKS];
        // A ring of points of radius 80 crop px around (128, 128), plus the
        // eye corners along the tilted eye line.
        for i in 0..LANDMARKS {
            let a = i as f32 / LANDMARKS as f32 * std::f32::consts::TAU;
            l[3 * i] = 128.0 + 80.0 * a.cos();
            l[3 * i + 1] = 128.0 + 80.0 * a.sin();
        }
        let (s, c) = tilt.sin_cos();
        for (idx, d) in [(RIGHT_EYE[0], -40.0f32), (LEFT_EYE[3], 40.0)] {
            l[3 * idx] = 128.0 + d * c;
            l[3 * idx + 1] = 128.0 + d * s;
        }
        let next = crop.refined(&l).expect("re-crop");
        assert!((next.angle - tilt).abs() < 1e-3, "{}", next.angle.to_degrees());
        // Centred on the ring's centre, 1.5 x its diameter (in image px).
        assert!((next.cx - 500.0).abs() < 1.0 && (next.cy - 400.0).abs() < 1.0, "{next:?}");
        let diameter = 160.0 * 200.0 / 256.0;
        assert!((next.side - MESH_CROP_SCALE * diameter).abs() < 2.0, "{}", next.side);
        // Points map back through the new crop: its centre is the ring's.
        let [x, y] = next.image_point(128.0, 128.0);
        assert!((x - 500.0).abs() < 1.0 && (y - 400.0).abs() < 1.0);
        // A level eye line (< 8°) keeps the first crop.
        for (idx, d) in [(RIGHT_EYE[0], -40.0f32), (LEFT_EYE[3], 40.0)] {
            l[3 * idx] = 128.0 + d;
            l[3 * idx + 1] = 128.0 + d * 0.1;
        }
        assert!(crop.refined(&l).is_none());
    }
}
