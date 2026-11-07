//! Working images: Rec. 709 luma, area-averaged downscaling.
//!
//! Every measure of this crate runs on a single-channel `f32` image whose
//! values are gamma-encoded luma in `[0, 255]`. Downscaling uses exact area
//! averaging (each output pixel is the mean of the source pixels it covers,
//! fractional coverage included): it is fast, has no ringing, and gives the
//! same optical blur to every photo whatever its preview size, which keeps
//! sharpness values comparable inside a project.

use image::RgbImage;

/// Long edge of the working image used by [`crate::compute_metrics`].
pub(crate) const WORK_LONG_EDGE: usize = 1024;

/// Single-channel `f32` image, row-major, values in `[0, 255]`.
#[derive(Debug, Clone, Default)]
pub(crate) struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<f32>,
}

impl Gray {
    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }
}

/// Output size so that the long edge is at most `max_long` (never upscales).
pub(crate) fn fit_size(w: usize, h: usize, max_long: usize) -> (usize, usize) {
    let long = w.max(h);
    if long <= max_long || long == 0 {
        return (w, h);
    }
    let scale = max_long as f64 / long as f64;
    let dw = ((w as f64 * scale).round() as usize).clamp(1, max_long);
    let dh = ((h as f64 * scale).round() as usize).clamp(1, max_long);
    (dw, dh)
}

/// Luma of the rectangle `[x0, x1) × [y0, y1)` of `img`, area-downscaled so
/// its long edge is at most `max_long`.
///
/// When the reduction is at least 2× on both axes, a 2×2 box average is done
/// first in integer arithmetic (vectorisable, one read of the RGB bytes),
/// then the remaining fractional area resample. The extra blur of the two
/// stages versus one exact area resample is ~0.06 px at the output scale.
pub(crate) fn luma_rect(img: &RgbImage, x0: usize, y0: usize, x1: usize, y1: usize, max_long: usize) -> Gray {
    let (iw, ih) = (img.width() as usize, img.height() as usize);
    let (x1, y1) = (x1.min(iw), y1.min(ih));
    if x0 >= x1 || y0 >= y1 {
        return Gray::default();
    }
    let (sw, sh) = (x1 - x0, y1 - y0);
    let (dw, dh) = fit_size(sw, sh, max_long);
    let raw: &[u8] = img.as_raw();
    let stride = iw * 3;
    if sw >= 2 * dw && sh >= 2 * dh && sw >= 2 && sh >= 2 {
        let half = luma_box2(raw, stride, x0, y0, sw, sh);
        if half.w == dw && half.h == dh {
            return half;
        }
        return resize_exact(&half, dw, dh);
    }
    let fill = |y: usize, row: &mut [f32]| {
        let start = (y0 + y) * stride + x0 * 3;
        let src = &raw[start..start + sw * 3];
        for (o, p) in row.iter_mut().zip(src.as_chunks::<3>().0) {
            *o = (54 * p[0] as u32 + 183 * p[1] as u32 + 19 * p[2] as u32) as f32 * (1.0 / 256.0);
        }
    };
    if dw == sw && dh == sh {
        let mut px = vec![0f32; sw * sh];
        for (y, row) in px.chunks_exact_mut(sw).enumerate() {
            fill(y, row);
        }
        return Gray { w: sw, h: sh, px };
    }
    Gray { w: dw, h: dh, px: area_resample(sw, sh, dw, dh, fill) }
}

/// 2×2 box average of the luma of a rectangle (odd last row/column dropped).
/// Integer Rec. 709 weights 54/183/19 (sum 256).
fn luma_box2(raw: &[u8], stride: usize, x0: usize, y0: usize, sw: usize, sh: usize) -> Gray {
    let (w, h) = (sw / 2, sh / 2);
    let mut px = vec![0f32; w * h];
    for (y, out) in px.chunks_exact_mut(w).enumerate() {
        let s0 = (y0 + 2 * y) * stride + x0 * 3;
        let s1 = s0 + stride;
        let (r0, r1) = (&raw[s0..s0 + w * 6], &raw[s1..s1 + w * 6]);
        for ((o, a), b) in out.iter_mut().zip(r0.as_chunks::<6>().0).zip(r1.as_chunks::<6>().0) {
            let r = a[0] as u32 + a[3] as u32 + b[0] as u32 + b[3] as u32;
            let g = a[1] as u32 + a[4] as u32 + b[1] as u32 + b[4] as u32;
            let bl = a[2] as u32 + a[5] as u32 + b[2] as u32 + b[5] as u32;
            *o = (54 * r + 183 * g + 19 * bl) as f32 * (1.0 / 1024.0);
        }
    }
    Gray { w, h, px }
}

/// Whole image to luma, long edge at most `max_long`.
pub(crate) fn luma_fit(img: &RgbImage, max_long: usize) -> Gray {
    luma_rect(img, 0, 0, img.width() as usize, img.height() as usize, max_long)
}

/// Area resample of a gray image to an exact size (used for hashes; works
/// for upscaling too, as nearest-neighbour boxes).
pub(crate) fn resize_exact(g: &Gray, dw: usize, dh: usize) -> Gray {
    if g.is_empty() || dw == 0 || dh == 0 {
        return Gray::default();
    }
    let w = g.w;
    let px = area_resample(g.w, g.h, dw, dh, |y, row| {
        row.copy_from_slice(&g.px[y * w..(y + 1) * w]);
    });
    Gray { w: dw, h: dh, px }
}

/// Area weights along one axis: for each output sample, the first source
/// sample and `taps` weights (zero padded, `start + taps <= src`).
struct AxisWeights {
    start: Vec<usize>,
    taps: usize,
    weights: Vec<f32>,
}

impl AxisWeights {
    fn new(src: usize, dst: usize) -> AxisWeights {
        let f = src as f64 / dst as f64;
        let taps = (f.ceil() as usize + 1).clamp(1, src);
        let mut start = Vec::with_capacity(dst);
        let mut weights = vec![0f32; dst * taps];
        for i in 0..dst {
            let a = i as f64 * f;
            let b = ((i + 1) as f64 * f).min(src as f64);
            let first = (a.floor() as usize).min(src - taps);
            start.push(first);
            let mut total = 0f64;
            for k in 0..taps {
                let j = first + k;
                let lo = (j as f64).max(a);
                let hi = ((j + 1) as f64).min(b);
                if hi > lo {
                    weights[i * taps + k] = (hi - lo) as f32;
                    total += hi - lo;
                }
            }
            if total > 0.0 {
                for k in 0..taps {
                    weights[i * taps + k] /= total as f32;
                }
            }
        }
        AxisWeights { start, taps, weights }
    }

    #[inline]
    fn apply(&self, row: &[f32], out: &mut [f32]) {
        let taps = self.taps;
        if taps == 3 {
            for ((o, &s), w) in out.iter_mut().zip(&self.start).zip(self.weights.as_chunks::<3>().0) {
                let r = &row[s..s + 3];
                *o = r[0] * w[0] + r[1] * w[1] + r[2] * w[2];
            }
            return;
        }
        for ((o, &s), w) in out.iter_mut().zip(&self.start).zip(self.weights.chunks_exact(taps)) {
            *o = row[s..s + taps].iter().zip(w).map(|(a, b)| a * b).sum();
        }
    }
}

/// Separable exact area resampling. `src_row(y, buf)` fills source row `y`.
/// Streams the source once: rows are accumulated vertically at full width
/// (contiguous, vectorised), and each finished output row is resampled
/// horizontally.
fn area_resample<F: FnMut(usize, &mut [f32])>(sw: usize, sh: usize, dw: usize, dh: usize, mut src_row: F) -> Vec<f32> {
    let hw = AxisWeights::new(sw, dw);
    let fy = sh as f64 / dh as f64;
    // Output rows a source row can touch at once.
    let ring = ((1.0 / fy).ceil() as usize + 2).max(3);
    let mut acc = vec![vec![0f32; sw]; ring];
    let mut out = vec![0f32; dw * dh];
    let mut row = vec![0f32; sw];
    let mut next = 0usize;
    let flush = |oy: usize, acc: &mut Vec<Vec<f32>>, out: &mut Vec<f32>| {
        let a = &mut acc[oy % ring];
        hw.apply(a, &mut out[oy * dw..(oy + 1) * dw]);
        a.iter_mut().for_each(|v| *v = 0.0);
    };
    for j in 0..sh {
        src_row(j, &mut row);
        let y0 = j as f64;
        let y1 = y0 + 1.0;
        let mut oy = ((y0 / fy).floor() as usize).min(dh - 1);
        while oy < dh && (oy as f64 * fy) < y1 {
            let a = (oy as f64 * fy).max(y0);
            let b = ((oy + 1) as f64 * fy).min(y1);
            if b > a {
                let w = ((b - a) / fy) as f32;
                for (o, v) in acc[oy % ring].iter_mut().zip(&row) {
                    *o += w * v;
                }
            }
            oy += 1;
        }
        // Output rows entirely covered by source rows 0..=j are final.
        while next < dh && ((next + 1) as f64 * fy) <= y1 + 1e-9 {
            flush(next, &mut acc, &mut out);
            next += 1;
        }
    }
    while next < dh {
        flush(next, &mut acc, &mut out);
        next += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_aspect() {
        assert_eq!(fit_size(3000, 2000, 1024), (1024, 683));
        assert_eq!(fit_size(2000, 3000, 1024), (683, 1024));
        assert_eq!(fit_size(800, 600, 1024), (800, 600));
        assert_eq!(fit_size(100_000, 10, 1024), (1024, 1));
    }

    #[test]
    fn area_resample_preserves_mean() {
        let img = RgbImage::from_fn(301, 207, |x, y| {
            let v = ((x * 7 + y * 13) % 256) as u8;
            image::Rgb([v, v, v])
        });
        let full = luma_fit(&img, 10_000);
        let small = luma_fit(&img, 64);
        assert_eq!(small.w.max(small.h), 64);
        let m1: f32 = full.px.iter().sum::<f32>() / full.px.len() as f32;
        let m2: f32 = small.px.iter().sum::<f32>() / small.px.len() as f32;
        assert!((m1 - m2).abs() < 0.5, "{m1} vs {m2}");
    }

    #[test]
    fn constant_stays_constant() {
        let img = RgbImage::from_pixel(1500, 1000, image::Rgb([100, 150, 200]));
        let g = luma_fit(&img, 1024);
        let expect = (54.0 * 100.0 + 183.0 * 150.0 + 19.0 * 200.0) / 256.0;
        assert!(g.px.iter().all(|v| (v - expect).abs() < 1e-2));
        let tiny = resize_exact(&g, 9, 8);
        assert!(tiny.px.iter().all(|v| (v - expect).abs() < 1e-2));
        let up = resize_exact(&Gray { w: 3, h: 2, px: vec![1.0; 6] }, 32, 32);
        assert!(up.px.iter().all(|v| (v - 1.0).abs() < 1e-5));
    }
}
