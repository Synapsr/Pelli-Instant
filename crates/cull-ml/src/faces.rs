//! Face detection with YuNet 2023mar (OpenCV Zoo, MIT).
//!
//! Input: BGR, `0..255` floats, NCHW, H and W multiples of 32 (zero padded
//! bottom/right). Outputs per stride s in {8, 16, 32}: `cls_s`, `obj_s`
//! (`[1, n, 1]`), `bbox_s` (`[1, n, 4]`), `kps_s` (`[1, n, 10]`) with
//! `n = (H/s) * (W/s)` anchors in row-major order. Decoding (as OpenCV's
//! `FaceDetectorYN`): `score = sqrt(clamp(cls) * clamp(obj))`,
//! `cx = (col + dx) * s`, `cy = (row + dy) * s`, `w = exp(dw) * s`,
//! `h = exp(dh) * s`, landmark `j = ((col + kx_j) * s, (row + ky_j) * s)`,
//! then greedy NMS.
//!
//! Passes: the whole photo at 640 px on its long side (≈6 ms on one core);
//! when it finds small faces or a group, a second pass at 1280 px (≈4x the
//! cost) recovers faces down to ~10 px at that scale, i.e. ~1 % of the
//! photo's long side; when it finds nothing, a 320 px pass (≈2 ms) catches
//! faces filling a close-up (YuNet is trained for ~10–300 px faces).

use crate::imgproc::{fit_long_side, resize, RgbView};
use crate::runtime::{output_f32, SessionPool};
use crate::{MlError, Result};
use fast_image_resize::{FilterType, ResizeAlg};
use ort::value::TensorRef;

/// Minimum detector score kept.
pub(crate) const SCORE_THRESHOLD: f32 = 0.6;
const NMS_IOU: f32 = 0.3;
const MAX_CANDIDATES: usize = 2000;
/// Long side of the first pass.
const BASE_LONG: u32 = 640;
/// Long side of the coarse pass run when the first pass finds nothing: YuNet
/// is trained for faces of ~10–300 px, a face filling a close-up exceeds
/// that at 640 px.
const COARSE_LONG: u32 = 320;
/// Long side of the second (fine) pass.
pub(crate) const FINE_LONG: u32 = 1280;
/// A face shorter than this (pixels at the 640 px base scale) triggers the
/// fine pass.
const SMALL_FACE_BASE_PX: f32 = 40.0;
/// This many faces (a group) triggers the fine pass too.
const GROUP_FACES: usize = 3;

/// One detected face, in full-resolution pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Face {
    pub score: f32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Right eye, left eye, nose, right mouth corner, left mouth corner.
    pub kps: [[f32; 2]; 5],
}

impl Face {
    fn scaled(mut self, sx: f32, sy: f32) -> Face {
        self.x *= sx;
        self.y *= sy;
        self.w *= sx;
        self.h *= sy;
        for k in &mut self.kps {
            k[0] *= sx;
            k[1] *= sy;
        }
        self
    }

    pub(crate) fn iod(&self) -> f32 {
        let [r, l] = [self.kps[0], self.kps[1]];
        ((l[0] - r[0]).powi(2) + (l[1] - r[1]).powi(2)).sqrt()
    }
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

/// Greedy non-maximum suppression, highest score first.
pub(crate) fn nms(mut faces: Vec<Face>, iou_threshold: f32) -> Vec<Face> {
    faces.sort_by(|a, b| b.score.total_cmp(&a.score));
    faces.truncate(MAX_CANDIDATES);
    let mut keep: Vec<Face> = Vec::new();
    for f in faces {
        if keep.iter().all(|k| iou(k, &f) <= iou_threshold) {
            keep.push(f);
        }
    }
    keep
}

pub(crate) struct FaceDetector {
    pub pool: SessionPool,
}

impl FaceDetector {
    /// Run one YuNet pass on `img`; coordinates in `img` pixels.
    fn run(&self, img: RgbView<'_>) -> Result<Vec<Face>> {
        let w = img.w as usize;
        let h = img.h as usize;
        let pw = w.div_ceil(32) * 32;
        let ph = h.div_ceil(32) * 32;
        let plane = pw * ph;
        let mut input = vec![0f32; 3 * plane];
        for y in 0..h {
            let row = &img.data[y * w * 3..(y + 1) * w * 3];
            for (x, px) in row.chunks_exact(3).enumerate() {
                let i = y * pw + x;
                input[i] = px[2] as f32; // B
                input[plane + i] = px[1] as f32; // G
                input[2 * plane + i] = px[0] as f32; // R
            }
        }
        self.pool.with(|session| {
            let tensor = TensorRef::from_array_view(([1usize, 3, ph, pw], &input[..]))
                .map_err(|e| MlError::Runtime(format!("yunet input: {e}")))?;
            let outputs = session.run(ort::inputs![tensor]).map_err(|e| MlError::Runtime(format!("yunet: {e}")))?;
            let mut faces = Vec::new();
            for stride in [8usize, 16, 32] {
                let cols = pw / stride;
                let rows = ph / stride;
                let n = cols * rows;
                let cls = output_f32(&outputs, &format!("cls_{stride}"), n)?;
                let obj = output_f32(&outputs, &format!("obj_{stride}"), n)?;
                let bbox = output_f32(&outputs, &format!("bbox_{stride}"), 4 * n)?;
                let kps = output_f32(&outputs, &format!("kps_{stride}"), 10 * n)?;
                let s = stride as f32;
                for i in 0..n {
                    let score = (cls[i].clamp(0.0, 1.0) * obj[i].clamp(0.0, 1.0)).sqrt();
                    if score.is_nan() || score < SCORE_THRESHOLD {
                        continue;
                    }
                    let (r, c) = ((i / cols) as f32, (i % cols) as f32);
                    let b = &bbox[4 * i..4 * i + 4];
                    let cx = (c + b[0]) * s;
                    let cy = (r + b[1]) * s;
                    let bw = b[2].clamp(-10.0, 10.0).exp() * s;
                    let bh = b[3].clamp(-10.0, 10.0).exp() * s;
                    let k = &kps[10 * i..10 * i + 10];
                    let mut pts = [[0f32; 2]; 5];
                    for (j, p) in pts.iter_mut().enumerate() {
                        *p = [(c + k[2 * j]) * s, (r + k[2 * j + 1]) * s];
                    }
                    let face = Face { score, x: cx - bw / 2.0, y: cy - bh / 2.0, w: bw, h: bh, kps: pts };
                    if [face.x, face.y, face.w, face.h].iter().all(|v| v.is_finite())
                        && pts.iter().flatten().all(|v| v.is_finite())
                    {
                        faces.push(face);
                    }
                }
            }
            Ok(nms(faces, NMS_IOU))
        })
    }

    /// Detect faces of the full-resolution photo `full` (`work` is the same
    /// photo with its long side ≤ [`FINE_LONG`]). Coordinates in `full`
    /// pixels, sorted by decreasing area.
    pub(crate) fn detect(&self, full: RgbView<'_>, work: RgbView<'_>) -> Result<Vec<Face>> {
        let (bw, bh) = fit_long_side(work.w, work.h, BASE_LONG);
        let base_buf;
        let base = if (bw, bh) == (work.w, work.h) {
            work
        } else {
            base_buf = resize(work, bw, bh, ResizeAlg::Convolution(FilterType::Bilinear))?;
            base_buf.view()
        };
        let (bsx, bsy) = (full.w as f32 / base.w as f32, full.h as f32 / base.h as f32);
        let mut faces: Vec<Face> = self.run(base)?.into_iter().map(|f| f.scaled(bsx, bsy)).collect();

        if faces.is_empty() && base.w.max(base.h) > COARSE_LONG {
            let (cw, ch) = fit_long_side(base.w, base.h, COARSE_LONG);
            let coarse = resize(base, cw, ch, ResizeAlg::Convolution(FilterType::Bilinear))?;
            let (csx, csy) = (full.w as f32 / cw as f32, full.h as f32 / ch as f32);
            // Only large faces are trusted at this scale.
            faces = self.run(coarse.view())?.into_iter().filter(|f| f.h >= 64.0).map(|f| f.scaled(csx, csy)).collect();
        }

        let fine_gain = work.w.max(work.h) as f32 / base.w.max(base.h) as f32;
        let wants_fine = faces.len() >= GROUP_FACES || faces.iter().any(|f| f.h / bsy < SMALL_FACE_BASE_PX);
        if fine_gain >= 1.25 && wants_fine {
            let (wsx, wsy) = (full.w as f32 / work.w as f32, full.h as f32 / work.h as f32);
            faces.extend(self.run(work)?.into_iter().map(|f| f.scaled(wsx, wsy)));
            faces = nms(faces, NMS_IOU);
        }
        faces.sort_by(|a, b| (b.w * b.h).total_cmp(&(a.w * a.h)));
        Ok(faces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(score: f32, x: f32, y: f32, s: f32) -> Face {
        Face { score, x, y, w: s, h: s, kps: [[0.0; 2]; 5] }
    }

    #[test]
    fn nms_keeps_best_of_overlapping() {
        let out = nms(vec![f(0.7, 0.0, 0.0, 10.0), f(0.9, 1.0, 1.0, 10.0), f(0.8, 50.0, 50.0, 10.0)], 0.3);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].score, 0.9);
        assert_eq!(out[1].score, 0.8);
    }

    #[test]
    fn iod_of_level_eyes() {
        let mut face = f(1.0, 0.0, 0.0, 10.0);
        face.kps[0] = [10.0, 20.0];
        face.kps[1] = [40.0, 20.0];
        assert_eq!(face.iod(), 30.0);
    }
}
