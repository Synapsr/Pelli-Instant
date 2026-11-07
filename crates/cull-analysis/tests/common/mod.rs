//! Test helpers: synthetic scenes and photographic degradations (blur, motion
//! blur, noise, exposure shifts, resize, JPEG recompression, crops).
//!
//! Shared by the integration tests and `examples/calibrate.rs` (included with
//! `#[path]`), so the calibration table and the tests degrade photos the same
//! way.

#![allow(dead_code)]

use image::{Rgb, RgbImage};

/// xorshift64* — deterministic, dependency-free.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.f32()
    }

    /// Standard normal (Box-Muller).
    pub fn normal(&mut self) -> f32 {
        let u1 = self.f32().max(1e-7);
        let u2 = self.f32();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
    }
}

/// A synthetic "natural" scene: overlapping anti-aliased discs, rotated
/// rectangles and thin lines of random tones (kept in 30..225 so nothing
/// clips) over a soft gradient, plus a faint micro-texture.
pub fn scene(w: u32, h: u32, seed: u64) -> RgbImage {
    let mut rng = Rng::new(seed);
    let (wf, hf) = (w as f32, h as f32);
    let base: [f32; 3] = [rng.range(80.0, 160.0), rng.range(80.0, 160.0), rng.range(80.0, 160.0)];
    let mut px: Vec<[f32; 3]> = (0..(w * h) as usize)
        .map(|i| {
            let (x, y) = ((i as u32 % w) as f32 / wf, (i as u32 / w) as f32 / hf);
            let g = 0.8 + 0.4 * (0.6 * x + 0.4 * y);
            [base[0] * g, base[1] * g, base[2] * g]
        })
        .collect();
    let scale = wf.max(hf);
    enum Shape {
        Disc { cx: f32, cy: f32, r: f32 },
        Rect { cx: f32, cy: f32, hw: f32, hh: f32, cos: f32, sin: f32 },
    }
    for k in 0..90 {
        let tone = [rng.range(35.0, 220.0), rng.range(35.0, 220.0), rng.range(35.0, 220.0)];
        let size = scale * rng.range(0.015, 0.12) * if k < 10 { 2.0 } else { 1.0 };
        let (cx, cy) = (rng.range(0.0, wf), rng.range(0.0, hf));
        let shape = if k % 3 == 0 {
            Shape::Disc { cx, cy, r: size }
        } else {
            let a = rng.range(0.0, std::f32::consts::PI);
            let thin = k % 7 == 0;
            Shape::Rect {
                cx,
                cy,
                hw: size,
                hh: if thin { scale * 0.0015 + 0.6 } else { size * rng.range(0.2, 1.0) },
                cos: a.cos(),
                sin: a.sin(),
            }
        };
        let (x0, x1, y0, y1) = match shape {
            Shape::Disc { cx, cy, r } => (cx - r, cx + r, cy - r, cy + r),
            Shape::Rect { cx, cy, hw, hh, .. } => {
                let e = hw.max(hh);
                (cx - e, cx + e, cy - e, cy + e)
            }
        };
        let (x0, x1) = ((x0.floor().max(0.0)) as u32, (x1.ceil().min(wf - 1.0)) as u32);
        let (y0, y1) = ((y0.floor().max(0.0)) as u32, (y1.ceil().min(hf - 1.0)) as u32);
        for y in y0..=y1 {
            for x in x0..=x1 {
                // 4×4 supersampling for anti-aliased (ideally sharp) edges.
                let mut cover = 0u32;
                for sy in 0..4 {
                    for sx in 0..4 {
                        let (px_, py_) = (x as f32 + (sx as f32 + 0.5) / 4.0, y as f32 + (sy as f32 + 0.5) / 4.0);
                        let inside = match shape {
                            Shape::Disc { cx, cy, r } => (px_ - cx).powi(2) + (py_ - cy).powi(2) <= r * r,
                            Shape::Rect { cx, cy, hw, hh, cos, sin } => {
                                let (dx, dy) = (px_ - cx, py_ - cy);
                                let (u, v) = (dx * cos + dy * sin, -dx * sin + dy * cos);
                                u.abs() <= hw && v.abs() <= hh
                            }
                        };
                        cover += inside as u32;
                    }
                }
                if cover > 0 {
                    let a = cover as f32 / 16.0;
                    let p = &mut px[(y * w + x) as usize];
                    for c in 0..3 {
                        p[c] = p[c] * (1.0 - a) + tone[c] * a;
                    }
                }
            }
        }
    }
    let mut out = RgbImage::new(w, h);
    for (i, p) in out.pixels_mut().enumerate() {
        let t = 3.0 * rng.normal();
        *p = Rgb([q(px[i][0] + t), q(px[i][1] + t), q(px[i][2] + t)]);
    }
    out
}

fn q(v: f32) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

fn to_planes(img: &RgbImage) -> [Vec<f32>; 3] {
    let mut p = [Vec::new(), Vec::new(), Vec::new()];
    for c in 0..3 {
        p[c] = img.pixels().map(|px| px[c] as f32).collect();
    }
    p
}

fn from_planes(w: u32, h: u32, p: &[Vec<f32>; 3]) -> RgbImage {
    let mut out = RgbImage::new(w, h);
    for (i, px) in out.pixels_mut().enumerate() {
        *px = Rgb([q(p[0][i]), q(p[1][i]), q(p[2][i])]);
    }
    out
}

fn blur_plane(p: &[f32], w: usize, h: usize, sigma: f32) -> Vec<f32> {
    if sigma <= 0.0 {
        return p.to_vec();
    }
    let r = (3.0 * sigma).ceil() as isize;
    let k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let ks: f32 = k.iter().sum();
    let k: Vec<f32> = k.iter().map(|v| v / ks).collect();
    let mut tmp = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0;
            for (j, kv) in k.iter().enumerate() {
                let xx = (x as isize + j as isize - r).clamp(0, w as isize - 1) as usize;
                acc += kv * p[y * w + xx];
            }
            tmp[y * w + x] = acc;
        }
    }
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0;
            for (j, kv) in k.iter().enumerate() {
                let yy = (y as isize + j as isize - r).clamp(0, h as isize - 1) as usize;
                acc += kv * tmp[yy * w + x];
            }
            out[y * w + x] = acc;
        }
    }
    out
}

/// Gaussian (defocus-like) blur of standard deviation `sigma` px.
pub fn gaussian_blur(img: &RgbImage, sigma: f32) -> RgbImage {
    let (w, h) = img.dimensions();
    let p = to_planes(img);
    let b = [
        blur_plane(&p[0], w as usize, h as usize, sigma),
        blur_plane(&p[1], w as usize, h as usize, sigma),
        blur_plane(&p[2], w as usize, h as usize, sigma),
    ];
    from_planes(w, h, &b)
}

/// Linear motion blur: average along a `length` px segment at `angle_deg`.
pub fn motion_blur(img: &RgbImage, length: f32, angle_deg: f32) -> RgbImage {
    let (w, h) = img.dimensions();
    let p = to_planes(img);
    let n = (length.ceil() as usize * 2).max(2) + 1;
    let (dx, dy) = (angle_deg.to_radians().cos(), angle_deg.to_radians().sin());
    let sample = |plane: &[f32], x: f32, y: f32| -> f32 {
        let x = x.clamp(0.0, (w - 1) as f32);
        let y = y.clamp(0.0, (h - 1) as f32);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w as usize - 1), (y0 + 1).min(h as usize - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let at = |xx: usize, yy: usize| plane[yy * w as usize + xx];
        (at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx) * (1.0 - fy) + (at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx) * fy
    };
    let mut out = [vec![0f32; (w * h) as usize], vec![0f32; (w * h) as usize], vec![0f32; (w * h) as usize]];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            for c in 0..3 {
                let mut acc = 0.0;
                for s in 0..n {
                    let t = (s as f32 / (n - 1) as f32 - 0.5) * length;
                    acc += sample(&p[c], x as f32 + t * dx, y as f32 + t * dy);
                }
                out[c][i] = acc / n as f32;
            }
        }
    }
    from_planes(w, h, &out)
}

/// Additive Gaussian noise (same on the three channels, like luma grain).
pub fn add_noise(img: &RgbImage, sigma: f32, seed: u64) -> RgbImage {
    let mut rng = Rng::new(seed);
    let mut out = img.clone();
    for p in out.pixels_mut() {
        let n = sigma * rng.normal();
        for c in 0..3 {
            p[c] = q(p[c] as f32 + n);
        }
    }
    out
}

fn to_lin(v: f32) -> f32 {
    let v = v / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(l: f32) -> f32 {
    let l = l.clamp(0.0, 1.0);
    255.0 * if l <= 0.003_130_8 { 12.92 * l } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 }
}

/// Exposure change of `ev` stops in linear light, clipped like a sensor.
pub fn shift_ev(img: &RgbImage, ev: f32) -> RgbImage {
    let gain = 2f32.powf(ev);
    let lut: Vec<u8> = (0..256).map(|v| q(to_srgb(to_lin(v as f32) * gain))).collect();
    let mut out = img.clone();
    for p in out.pixels_mut() {
        for c in 0..3 {
            p[c] = lut[p[c] as usize];
        }
    }
    out
}

pub fn resize(img: &RgbImage, w: u32, h: u32) -> RgbImage {
    image::imageops::resize(img, w.max(1), h.max(1), image::imageops::FilterType::CatmullRom)
}

pub fn jpeg_roundtrip(img: &RgbImage, quality: u8) -> RgbImage {
    let mut buf = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, quality);
    enc.encode_image(img).expect("jpeg encode");
    image::load_from_memory(&buf).expect("jpeg decode").to_rgb8()
}

pub fn crop(img: &RgbImage, x: u32, y: u32, w: u32, h: u32) -> RgbImage {
    image::imageops::crop_imm(img, x, y, w, h).to_image()
}

/// Shallow depth of field: keep a centred ellipse covering `fraction` of
/// each dimension sharp, blur the rest with `sigma`, soft transition.
pub fn shallow_dof(img: &RgbImage, fraction: f32, sigma: f32) -> RgbImage {
    let (w, h) = img.dimensions();
    let blurred = gaussian_blur(img, sigma);
    let mut out = img.clone();
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let (rx, ry) = (w as f32 * fraction / 2.0, h as f32 * fraction / 2.0);
    for (x, y, p) in out.enumerate_pixels_mut() {
        let d = (((x as f32 - cx) / rx).powi(2) + ((y as f32 - cy) / ry).powi(2)).sqrt();
        let a = ((d - 0.85) / 0.3).clamp(0.0, 1.0); // 0 = sharp, 1 = blurred
        let b = blurred.get_pixel(x, y);
        for c in 0..3 {
            p[c] = q(p[c] as f32 * (1.0 - a) + b[c] as f32 * a);
        }
    }
    out
}

/// Smooth, nearly textureless image (sky / plain wall) with faint noise.
pub fn smooth_sky(w: u32, h: u32, seed: u64) -> RgbImage {
    let mut rng = Rng::new(seed);
    RgbImage::from_fn(w, h, |x, y| {
        let t = y as f32 / h as f32;
        let n = 1.0 * rng.normal();
        let _ = x;
        Rgb([q(120.0 + 60.0 * t + n), q(160.0 + 40.0 * t + n), q(220.0 - 10.0 * t + n)])
    })
}
