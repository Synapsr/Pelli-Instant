//! Resizing and crop sampling on packed RGB8 buffers.

use crate::{MlError, Result};
use fast_image_resize as fr;
use std::cell::RefCell;

/// Borrowed packed RGB8 image.
#[derive(Clone, Copy)]
pub(crate) struct RgbView<'a> {
    pub w: u32,
    pub h: u32,
    pub data: &'a [u8],
}

impl<'a> RgbView<'a> {
    pub(crate) fn new(w: u32, h: u32, data: &'a [u8]) -> Option<RgbView<'a>> {
        let need = (w as usize).checked_mul(h as usize)?.checked_mul(3)?;
        (w > 0 && h > 0 && data.len() >= need).then_some(RgbView { w, h, data })
    }
}

/// Owned packed RGB8 image.
pub(crate) struct RgbBuf {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

impl RgbBuf {
    pub(crate) fn view(&self) -> RgbView<'_> {
        RgbView { w: self.w, h: self.h, data: &self.data }
    }
}

thread_local! {
    static RESIZER: RefCell<fr::Resizer> = RefCell::new(fr::Resizer::new());
}

fn fr_err(e: impl std::fmt::Display) -> MlError {
    MlError::Runtime(format!("resize: {e}"))
}

/// Resize the crop `(left, top, width, height)` (source pixels, may be
/// fractional) of `src` to `dw x dh`.
pub(crate) fn resize_crop(
    src: RgbView<'_>,
    crop: (f64, f64, f64, f64),
    dw: u32,
    dh: u32,
    alg: fr::ResizeAlg,
) -> Result<RgbBuf> {
    let dw = dw.max(1);
    let dh = dh.max(1);
    let src_img = fr::images::ImageRef::new(src.w, src.h, src.data, fr::PixelType::U8x3).map_err(fr_err)?;
    let mut dst = fr::images::Image::new(dw, dh, fr::PixelType::U8x3);
    let (l, t, w, h) = crop;
    let opts = fr::ResizeOptions::new().resize_alg(alg).crop(l, t, w.max(1.0), h.max(1.0));
    RESIZER.with(|r| r.borrow_mut().resize(&src_img, &mut dst, &opts)).map_err(fr_err)?;
    Ok(RgbBuf { w: dw, h: dh, data: dst.into_vec() })
}

/// Resize the whole image to `dw x dh`.
pub(crate) fn resize(src: RgbView<'_>, dw: u32, dh: u32, alg: fr::ResizeAlg) -> Result<RgbBuf> {
    resize_crop(src, (0.0, 0.0, src.w as f64, src.h as f64), dw, dh, alg)
}

/// Dimensions of `w x h` scaled so that its long side is at most `max_long`
/// (never upscaled), at least 1 px each.
pub(crate) fn fit_long_side(w: u32, h: u32, max_long: u32) -> (u32, u32) {
    let long = w.max(h);
    if long <= max_long || long == 0 {
        return (w.max(1), h.max(1));
    }
    let s = max_long as f64 / long as f64;
    (((w as f64 * s).round() as u32).max(1), ((h as f64 * s).round() as u32).max(1))
}

#[inline]
fn bilinear(img: RgbView<'_>, x: f32, y: f32) -> [f32; 3] {
    // Pixel centres at integer + 0.5; clamp to the border (replicate).
    let maxx = (img.w - 1) as f32;
    let maxy = (img.h - 1) as f32;
    let x = (x - 0.5).clamp(0.0, maxx);
    let y = (y - 0.5).clamp(0.0, maxy);
    let x0 = x.floor();
    let y0 = y.floor();
    let fx = x - x0;
    let fy = y - y0;
    let x0 = x0 as usize;
    let y0 = y0 as usize;
    let x1 = (x0 + 1).min(img.w as usize - 1);
    let y1 = (y0 + 1).min(img.h as usize - 1);
    let stride = img.w as usize * 3;
    let p =
        |xx: usize, yy: usize, c: usize| -> f32 { img.data.get(yy * stride + xx * 3 + c).copied().unwrap_or(0) as f32 };
    let mut out = [0f32; 3];
    for (c, o) in out.iter_mut().enumerate() {
        let a = p(x0, y0, c) + (p(x1, y0, c) - p(x0, y0, c)) * fx;
        let b = p(x0, y1, c) + (p(x1, y1, c) - p(x0, y1, c)) * fx;
        *o = a + (b - a) * fy;
    }
    out
}

/// Sample a square of `side` source pixels centred on `(cx, cy)` and
/// rotated by `angle` radians (the crop's x axis points along
/// `(cos a, sin a)` in the source) into an `out x out` planar RGB tensor in
/// `[0, 1]` (`dst.len() == 3 * out * out`).
///
/// When downscaling, the bounding box of the rotated square is first
/// reduced with the SIMD resizer to ~`out` pixels per `side` (area-correct
/// and ~20x cheaper than supersampling a 24 MP photo), then sampled
/// bilinearly.
pub(crate) fn sample_rotated_chw(
    img: RgbView<'_>,
    cx: f32,
    cy: f32,
    side: f32,
    angle: f32,
    out: usize,
    dst: &mut [f32],
) {
    if dst.len() < 3 * out * out || out == 0 || side.is_nan() || side <= 0.0 || !cx.is_finite() || !cy.is_finite() {
        return;
    }
    let ratio = side / out as f32;
    if ratio > 1.25 {
        let (sa, ca) = angle.sin_cos();
        let half = side * 0.5 * (sa.abs() + ca.abs()) + 2.0 * ratio;
        let x0 = (cx - half).floor().max(0.0);
        let y0 = (cy - half).floor().max(0.0);
        let x1 = (cx + half).ceil().min(img.w as f32);
        let y1 = (cy + half).ceil().min(img.h as f32);
        if x1 - x0 >= 1.0 && y1 - y0 >= 1.0 {
            let sw = (((x1 - x0) / ratio).round() as u32).max(1);
            let sh = (((y1 - y0) / ratio).round() as u32).max(1);
            let alg = fr::ResizeAlg::Convolution(fr::FilterType::Bilinear);
            if let Ok(small) = resize_crop(img, (x0 as f64, y0 as f64, (x1 - x0) as f64, (y1 - y0) as f64), sw, sh, alg)
            {
                let kx = sw as f32 / (x1 - x0);
                let ky = sh as f32 / (y1 - y0);
                let k = 0.5 * (kx + ky);
                sample_direct(small.view(), (cx - x0) * kx, (cy - y0) * ky, side * k, angle, out, dst);
                return;
            }
        }
    }
    sample_direct(img, cx, cy, side, angle, out, dst);
}

/// Bilinear sampling, 2x2 supersampled when slightly downscaling.
fn sample_direct(img: RgbView<'_>, cx: f32, cy: f32, side: f32, angle: f32, out: usize, dst: &mut [f32]) {
    let n = out * out;
    let ss = ((side / out as f32).ceil() as usize).clamp(1, 2);
    let (sa, ca) = angle.sin_cos();
    let step = side / out as f32;
    let inv = 1.0 / (ss * ss) as f32 / 255.0;
    for oy in 0..out {
        for ox in 0..out {
            let mut acc = [0f32; 3];
            for sy in 0..ss {
                let v = (oy as f32 + (sy as f32 + 0.5) / ss as f32) * step - side * 0.5;
                for sx in 0..ss {
                    let u = (ox as f32 + (sx as f32 + 0.5) / ss as f32) * step - side * 0.5;
                    let x = cx + u * ca - v * sa;
                    let y = cy + u * sa + v * ca;
                    let p = bilinear(img, x, y);
                    acc[0] += p[0];
                    acc[1] += p[1];
                    acc[2] += p[2];
                }
            }
            let i = oy * out + ox;
            dst[i] = acc[0] * inv;
            dst[n + i] = acc[1] * inv;
            dst[2 * n + i] = acc[2] * inv;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit() {
        assert_eq!(fit_long_side(6000, 4000, 1280), (1280, 853));
        assert_eq!(fit_long_side(400, 300, 1280), (400, 300));
        assert_eq!(fit_long_side(10000, 1, 640), (640, 1));
    }

    #[test]
    fn rotated_sampling_identity_and_rotation() {
        // 4x4 image, left half red, right half blue.
        let mut data = vec![0u8; 4 * 4 * 3];
        for y in 0..4 {
            for x in 0..4 {
                let i = (y * 4 + x) * 3;
                if x < 2 {
                    data[i] = 255;
                } else {
                    data[i + 2] = 255;
                }
            }
        }
        let v = RgbView::new(4, 4, &data).unwrap();
        let mut dst = vec![0f32; 3 * 4 * 4];
        sample_rotated_chw(v, 2.0, 2.0, 4.0, 0.0, 4, &mut dst);
        assert!(dst[0] > 0.99 && dst[2 * 16 + 3] > 0.99); // R at left, B at right
                                                          // Rotated by 180°: blue on the left.
        sample_rotated_chw(v, 2.0, 2.0, 4.0, std::f32::consts::PI, 4, &mut dst);
        assert!(dst[2 * 16] > 0.99 && dst[3] > 0.99);
    }

    #[test]
    fn downscaled_sampling_matches_direct() {
        // Smooth gradient 400x300: the pre-resized path must agree with
        // plain supersampled sampling.
        let (w, h) = (400u32, 300u32);
        let mut data = vec![0u8; (w * h * 3) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 3) as usize;
                data[i] = (x * 255 / w) as u8;
                data[i + 1] = (y * 255 / h) as u8;
                data[i + 2] = 128;
            }
        }
        let v = RgbView::new(w, h, &data).unwrap();
        let mut a = vec![0f32; 3 * 32 * 32];
        let mut b = vec![0f32; 3 * 32 * 32];
        sample_rotated_chw(v, 200.0, 150.0, 160.0, 0.3, 32, &mut a);
        sample_direct(v, 200.0, 150.0, 160.0, 0.3, 32, &mut b);
        let max = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0f32, f32::max);
        assert!(max < 0.03, "{max}");
        // Crops partly outside the image and degenerate inputs don't panic.
        sample_rotated_chw(v, 2.0, 2.0, 500.0, 1.0, 32, &mut a);
        sample_rotated_chw(v, f32::NAN, 2.0, 500.0, 1.0, 32, &mut a);
        sample_rotated_chw(v, 2.0, 2.0, 0.0, 1.0, 32, &mut a);
    }

    #[test]
    fn resize_works_on_odd_sizes() {
        let data = vec![128u8; 7 * 5 * 3];
        let v = RgbView::new(7, 5, &data).unwrap();
        let r = resize(v, 3, 2, fr::ResizeAlg::Convolution(fr::FilterType::CatmullRom)).unwrap();
        assert_eq!(r.data.len(), 3 * 2 * 3);
        assert!(r.data.iter().all(|&p| (127..=129).contains(&p)));
    }
}
