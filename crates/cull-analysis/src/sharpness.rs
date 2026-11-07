//! Focus measures.
//!
//! # Edge width, the measure behind every focus score
//!
//! Classic focus measures (variance of the Laplacian, Tenengrad, modified
//! Laplacian) are *energies*: they grow with contrast and texture, so a sharp
//! low-contrast portrait in soft window light scores lower than a blurred
//! street scene full of neon signs. They are still computed (raw `global` and
//! `peak` fields: noise-corrected variance of the Laplacian) but the
//! normalised score is built on a contrast-invariant quantity: the **width of
//! the edges**.
//!
//! For every edge pixel (local maximum of the gradient along its direction,
//! `EDGE_NOISE_K` noise standard deviations above the noise), the ratio
//! between the gradient of the image and the gradient of a copy re-blurred by
//! a known Gaussian (binomial 9, σ0 = √2 px) gives the blur of that edge: a
//! sharp edge loses a lot when re-blurred, an already blurred one almost
//! nothing (Zhuo & Sim, "Defocus map estimation from a single image", 2011).
//! The image gradient is taken after a light [1 2 1]/4 pre-blur, which halves
//! the grain in it; the re-blurred copy is therefore one binomial-11 pass.
//! The ratio is converted to an equivalent Gaussian blur radius `s` (px of the
//! 1024 px working image) through a table computed once by simulating these
//! exact discrete operators on ideal area-sampled edges, so `s` reads as "the
//! edges are spread over ±s px". A focused photo downscaled to 1024 px has
//! `s ≈ 0.3-0.6`; σ = 1.5 px of defocus reads ≈ 1.0-1.5, σ = 2.5 ≈ 2-2.4.
//!
//! * Noise: the noise of each operator (Sobel of the pre-blurred image, Sobel
//!   of the re-blurred one, Laplacian) is measured on its own response, from
//!   the flattest quarter of the image, so the estimate stays right for the
//!   correlated noise left by demosaicing, noise reduction, JPEG and
//!   resampling. Gradient energies are corrected by it and edges must stand
//!   above it, so high-ISO grain neither creates fake edges nor makes blurred
//!   edges look sharp.
//! * Clipping: edges touching blown highlights or crushed blacks are ignored
//!   (a blurred lamp clipped at 255 has an artificially hard border).
//! * Low texture: a frame with few edges (fog, sky, plain wall) gets a score
//!   pulled towards `UNKNOWN_FOCUS`: no evidence of blur is not blur.
//! * Shallow depth of field: the frame is cut in tiles of ~128 px; the score
//!   uses the `PEAK_TILES` sharpest tiles only, whatever the number of tiles,
//!   so a frame is blurry only when **nothing** in it is sharp: a sharp
//!   subject on creamy bokeh, a product detail whose sharp zone covers 3-5%
//!   of the frame, a ring at f/2 are sharp. (The sharpest 15% of the tiles
//!   were used before: when the sharp zone was smaller than that, the
//!   measure fell into the bokeh.)
//!
//! Score mapping (logistic in log-width): `score = 1 / (1 + (s / S50)^GAMMA)`
//! with `S50 = 1.0 px`, `GAMMA = 4`: s = 0.5 → 0.94, 0.8 → 0.71, 1.0 → 0.5,
//! 1.5 → 0.16, 2.0 → 0.06. Calibrated on 39 real photos and their blurred
//! variants (`examples/calibrate.rs`): originals ≥ 0.8, σ 1.5 ≈ 0.5,
//! σ 2.5 ≤ 0.1.
//!
//! # Motion blur
//!
//! Camera shake and subject motion spread edges along one direction only.
//! Edge widths are histogrammed per gradient orientation (4 sectors of 45°)
//! and compared between perpendicular sectors (0°/90°, 45°/135°, which
//! cancels the pixel-grid bias of diagonals), on the sharpest quarter of the
//! edges of each orientation. Two estimates of that signed log ratio must
//! agree in direction, and the smaller one is used:
//! * frame-wide;
//! * per tile, the sharpest half of the tiles holding both orientations
//!   (ranked by their sharper direction, which motion along the other one
//!   does not affect and which is large in bokeh) voting with their median,
//!   so a direction only counts when it is consistent across the frame.
//!
//! Defocus blurs all orientations alike (ratio ≈ 1). Camera shake leaves no
//! part of the frame sharp in both directions, so a few tiles that are (a
//! sharp face among long hair, a textured wall, fur) veto the estimate: the
//! anisotropy elsewhere is texture, not motion. The face focus measured by
//! `region_sharpness` and used by `assess` is the final word on the subject. The ratio maps linearly in log from `MOTION_RATIO_START`
//! (0) to `MOTION_RATIO_FULL` (1), times a confidence on the number of
//! voting tiles.

use crate::prep::Gray;
use std::sync::OnceLock;

/// Number of edge-width histogram bins (log-spaced).
pub(crate) const NB: usize = 48;
const S_MIN: f64 = 0.25;
const S_MAX: f64 = 8.0;

/// Width at which the focus score is 0.5 (px at 1024 px long edge).
pub(crate) const S50: f32 = 1.0;
/// Steepness of the log-width logistic.
pub(crate) const GAMMA: f32 = 4.0;
/// Score given when there is no measurable edge (no evidence either way).
pub(crate) const UNKNOWN_FOCUS: f32 = 0.6;

/// Edge pixels must exceed this many noise standard deviations (gradient of
/// the re-blurred image).
const EDGE_NOISE_K: f32 = 5.0;
/// Absolute minimum gradient of an edge (luma levels / px, re-blurred image).
const EDGE_ABS_MIN: f32 = 1.2;
/// Luma at or above (below) which a pixel is treated as clipped.
const CLIP_HIGH: f32 = 252.0;
const CLIP_LOW: f32 = 2.0;
/// Smallest image side analysed.
pub(crate) const MIN_SIDE: usize = 8;
/// Target tile size in px for the frame grid.
const TILE_PX: usize = 128;
/// Minimum edges in a tile for it to vote (absolute, and per tile pixel).
const TILE_MIN_EDGES: f32 = 24.0;
const TILE_MIN_EDGE_DENSITY: f32 = 0.002;
/// Number of sharpest valid tiles forming the "peak" region (≈5% of a 3:2
/// frame of 40 tiles).
const PEAK_TILES: usize = 2;
/// The confidence of the frame score counts the edges of this share of the
/// sharpest tiles (at least `PEAK_TILES`): evidence that the frame has
/// measurable structure, while the width comes from the sharpest tiles.
const PEAK_EVIDENCE_FRACTION: f32 = 0.15;
/// Edges for which confidence reaches 0.5 (frame peak / region).
const FRAME_CONF_EDGES: f32 = 80.0;
const REGION_CONF_EDGES: f32 = 25.0;
/// Quantile of the edge widths of a region used as its sharpness (eyelashes,
/// iris and eyelid lines are the sharpest fifth of an eye area in focus).
const REGION_QUANTILE: f64 = 0.20;
/// Directional check of a region: edges needed per orientation, and the
/// quantile compared (the sharpest tenth: a face's horizontal features are
/// mostly soft shading, but in focus some of them are always sharp).
const REGION_DIR_MIN_EDGES: u32 = 40;
const REGION_DIR_QUANTILE: f64 = 0.10;
/// Motion blur: quantile of the widths compared (sharpest quarter), minimum
/// edges per orientation (frame, tile), share of
/// the valid tiles (sharpest first) that vote, tiles for which confidence
/// reaches 0.5, and the anisotropy ratio where the motion value starts (0)
/// and saturates (1).
const MOTION_QUANTILE: f64 = 0.25;
const MOTION_MIN_EDGES: u32 = 40;
const MOTION_TILE_MIN_EDGES: u32 = 15;
const MOTION_TILE_SHARE: f32 = 0.5;
const MOTION_CONF_TILES: f32 = 3.0;
/// Veto of the motion estimate: tiles sharp in both directions of a pair.
const MOTION_SHARP_BOTH: f32 = 0.45;
const MOTION_VETO_TILES: usize = 2;
const MOTION_VETO_SHARE: f32 = 0.15;
pub(crate) const MOTION_RATIO_START: f32 = 1.35;
pub(crate) const MOTION_RATIO_FULL: f32 = 2.2;

/// For a zero-mean Gaussian, the 25th percentile of |x| is 0.3186 σ. The
/// noise of each operator is measured on its own response (flattest quarter
/// of the image), which stays right for the spatially correlated noise left
/// by demosaicing, noise reduction, JPEG and resampling.
const Q25_ABS_GAUSS: f32 = 0.318_639;

// ---------------------------------------------------------------------------
// Ratio → width table
// ---------------------------------------------------------------------------

const R2_MAX: f32 = 9.0;
const LUT_N: usize = 4096;

fn bin_ratio() -> f64 {
    (S_MAX / S_MIN).powf(1.0 / NB as f64)
}

/// Width at a fractional histogram position.
fn width_at(pos: f64) -> f32 {
    (S_MIN * bin_ratio().powf(pos)) as f32
}

fn erf(x: f64) -> f64 {
    // Abramowitz & Stegun 7.1.26 (|error| < 1.5e-7).
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let y = 1.0
        - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t + 0.254_829_592)
            * t
            * (-x * x).exp();
    sign * y
}

/// ∫_{-∞}^{t} Φ(u) du.
fn int_cdf(t: f64) -> f64 {
    let cdf = 0.5 * (1.0 + erf(t / std::f64::consts::SQRT_2));
    let pdf = (-0.5 * t * t).exp() / (2.0 * std::f64::consts::PI).sqrt();
    t * cdf + pdf
}

/// Mean gradient ratio |∇I'| / |∇(I' ⊗ binomial9)| at the edge pixel of an
/// ideal unit step blurred by a Gaussian of σ = `s`, area-sampled, averaged
/// over 8 sub-pixel phases, where I' is the image lightly pre-blurred with
/// [1 2 1]/4. Mirrors the 2D operators across a straight edge.
fn simulate_ratio(s: f64) -> f64 {
    const N: usize = 56;
    fn conv(p: &[f64], k: &[f64]) -> Vec<f64> {
        let r = (k.len() / 2) as isize;
        let n = p.len() as isize;
        (0..n)
            .map(|i| k.iter().enumerate().map(|(j, kj)| kj * p[(i + j as isize - r).clamp(0, n - 1) as usize]).sum())
            .collect()
    }
    let pre = [0.25, 0.5, 0.25];
    let b5 = [1.0 / 16.0, 4.0 / 16.0, 6.0 / 16.0, 4.0 / 16.0, 1.0 / 16.0];
    let mut acc = 0.0;
    for phase in 0..8 {
        let x0 = (N / 2) as f64 + phase as f64 / 8.0 - 0.5;
        let p: Vec<f64> = (0..N)
            .map(|i| {
                let i = i as f64;
                s * (int_cdf((i + 0.5 - x0) / s) - int_cdf((i - 0.5 - x0) / s))
            })
            .collect();
        let g = conv(&p, &pre);
        let b = conv(&conv(&g, &b5), &b5);
        let mut best = (0.0, 1.0);
        for i in 6..N - 6 {
            let gb = (b[i + 1] - b[i - 1]) * 0.5;
            if gb.abs() > best.0 {
                best = (gb.abs(), ((g[i + 1] - g[i - 1]) * 0.5).abs() / gb.abs());
            }
        }
        acc += best.1;
    }
    acc / 8.0
}

struct WidthLut {
    /// R² (uniform in [1, R2_MAX]) → histogram bin.
    bins: Vec<u8>,
}

fn width_lut() -> &'static WidthLut {
    static LUT: OnceLock<WidthLut> = OnceLock::new();
    LUT.get_or_init(|| {
        const M: usize = 400;
        let (lo, hi) = (0.1f64, 16.0f64);
        let s: Vec<f64> = (0..M).map(|j| lo * (hi / lo).powf(j as f64 / (M - 1) as f64)).collect();
        let mut r: Vec<f64> = s.iter().map(|&s| simulate_ratio(s)).collect();
        // Monotonic: the ratio may only decrease when the width grows.
        for j in (0..M - 1).rev() {
            r[j] = r[j].max(r[j + 1]);
        }
        let ln_q = bin_ratio().ln();
        let bins = (0..LUT_N)
            .map(|e| {
                let r2 = 1.0 + (e as f64 + 0.5) / LUT_N as f64 * (R2_MAX as f64 - 1.0);
                let ratio = r2.sqrt();
                let width = match r.iter().position(|&rj| rj <= ratio) {
                    None => hi,
                    Some(0) => s[0],
                    Some(j) => {
                        // Interpolate in log-width between j-1 and j.
                        let (ra, rb) = (r[j - 1], r[j]);
                        let t = if ra > rb { (ra - ratio) / (ra - rb) } else { 0.0 };
                        (s[j - 1].ln() + t * (s[j].ln() - s[j - 1].ln())).exp()
                    }
                };
                ((width / S_MIN).ln() / ln_q).floor().clamp(0.0, (NB - 1) as f64) as u8
            })
            .collect();
        WidthLut { bins }
    })
}

#[inline]
fn width_bin(lut: &WidthLut, r2: f32) -> usize {
    if r2 >= R2_MAX {
        0
    } else if r2 <= 1.0 {
        NB - 1
    } else {
        let e = ((r2 - 1.0) / (R2_MAX - 1.0) * LUT_N as f32) as usize;
        lut.bins[e.min(LUT_N - 1)] as usize
    }
}

/// Edge width (px) → focus score in `[0, 1]`.
pub(crate) fn width_to_score(s: f32) -> f32 {
    if !s.is_finite() {
        return 0.0;
    }
    1.0 / (1.0 + (s.max(0.0) / S50).powf(GAMMA))
}

// ---------------------------------------------------------------------------
// Edge analysis
// ---------------------------------------------------------------------------

pub(crate) type Hist = [u32; NB];

/// Width quantile of a histogram (log-interpolated inside the bin).
pub(crate) fn hist_quantile(h: &Hist, q: f64) -> Option<f32> {
    let n: u64 = h.iter().map(|&c| c as u64).sum();
    if n == 0 {
        return None;
    }
    let target = q.clamp(0.0, 1.0) * n as f64;
    let mut cum = 0.0;
    for (k, &c) in h.iter().enumerate() {
        let c = c as f64;
        if c > 0.0 && cum + c >= target {
            let frac = ((target - cum) / c).clamp(0.0, 1.0);
            return Some(width_at(k as f64 + frac));
        }
        cum += c;
    }
    Some(width_at(NB as f64))
}

pub(crate) struct EdgeAnalysis {
    pub nx: usize,
    pub ny: usize,
    pub tile_hist: Vec<Hist>,
    pub tile_edges: Vec<u32>,
    pub tile_px: Vec<u32>,
    /// Σ Laplacian² per tile.
    pub tile_lap: Vec<f64>,
    /// Σ Sobel magnitude² (Tenengrad) and Σ |Lxx| + |Lyy| per tile: classic
    /// measures, only filled when requested (benchmarks).
    pub tile_ten: Vec<f64>,
    pub tile_modlap: Vec<f64>,
    pub orient_hist: [Hist; 4],
    /// Edge widths per tile and orientation sector (motion blur).
    pub tile_orient: Vec<[Hist; 4]>,
    /// Noise standard deviations of: one Sobel component of the re-blurred
    /// image, one Sobel component of the image, the Laplacian.
    pub noise_blur_grad: f32,
    pub noise_grad: f32,
    pub noise_lap: f32,
}

impl EdgeAnalysis {
    pub fn total_edges(&self) -> u32 {
        self.tile_edges.iter().sum()
    }

    pub fn all_hist(&self) -> Hist {
        let mut h = [0u32; NB];
        for t in &self.tile_hist {
            for (a, b) in h.iter_mut().zip(t) {
                *a += b;
            }
        }
        h
    }

    /// Noise-corrected variance of the Laplacian of tile `t`.
    pub fn tile_vol(&self, t: usize) -> f32 {
        if self.tile_px[t] == 0 {
            return 0.0;
        }
        ((self.tile_lap[t] / self.tile_px[t] as f64) as f32 - self.noise_lap * self.noise_lap).max(0.0)
    }

    pub fn global_vol(&self) -> f32 {
        let px: u64 = self.tile_px.iter().map(|&p| p as u64).sum();
        if px == 0 {
            return 0.0;
        }
        let lap: f64 = self.tile_lap.iter().sum();
        ((lap / px as f64) as f32 - self.noise_lap * self.noise_lap).max(0.0)
    }

    /// Equivalent white-noise sigma of the image (luma levels), from the
    /// Laplacian noise (gain √20). Informational.
    pub fn noise_sigma(&self) -> f32 {
        self.noise_lap / 20f32.sqrt()
    }
}

/// Noise standard deviations of one Sobel component of the re-blurred
/// image, one Sobel component of the pre-blurred image, and the Laplacian.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Noise {
    pub blur_grad: f32,
    pub grad: f32,
    pub lap: f32,
}

/// Noise of an image (for a small region, measured on a larger context
/// around it: a 40 px face crop has no flat quarter to measure noise on).
pub(crate) fn estimate_noise(g: &Gray) -> Option<Noise> {
    analyze_inner(g, 1, 1, false, None, true).map(|a| Noise {
        blur_grad: a.noise_blur_grad,
        grad: a.noise_grad,
        lap: a.noise_lap,
    })
}

/// Histogram of |values| on a linear grid, for robust low quantiles.
struct AbsHist {
    inv_bin: f32,
    bin: f32,
    counts: Vec<u32>,
    n: u32,
}

impl AbsHist {
    fn new(bin: f32, bins: usize) -> AbsHist {
        AbsHist { inv_bin: 1.0 / bin, bin, counts: vec![0; bins + 1], n: 0 }
    }

    #[inline]
    fn add(&mut self, v: f32) {
        let k = ((v.abs() * self.inv_bin) as usize).min(self.counts.len() - 1);
        self.counts[k] += 1;
        self.n += 1;
    }

    /// Noise σ from the 25th percentile of |x|.
    fn sigma(&self) -> f32 {
        if self.n < 64 {
            return 0.0;
        }
        let target = self.n as f32 * 0.25;
        let mut cum = 0u32;
        for (k, &c) in self.counts.iter().enumerate() {
            if c > 0 && (cum + c) as f32 >= target {
                let frac = (target - cum as f32) / c as f32;
                return (k as f32 + frac) * self.bin / Q25_ABS_GAUSS;
            }
            cum += c;
        }
        0.0
    }
}

/// Separable [1 2 1]/4 blur (σ² = 0.5 px²), clamped borders.
fn binomial3(g: &Gray) -> Gray {
    let (w, h) = (g.w, g.h);
    let mut tmp = vec![0f32; w * h];
    for (src, dst) in g.px.chunks_exact(w).zip(tmp.chunks_exact_mut(w)) {
        if w >= 3 {
            let (a, b, c) = (&src[..w - 2], &src[1..w - 1], &src[2..]);
            for (k, o) in dst[1..w - 1].iter_mut().enumerate() {
                *o = (a[k] + 2.0 * b[k] + c[k]) * 0.25;
            }
        }
        dst[0] = (3.0 * src[0] + src[1.min(w - 1)]) * 0.25;
        dst[w - 1] = (3.0 * src[w - 1] + src[w.saturating_sub(2)]) * 0.25;
    }
    let mut out = vec![0f32; w * h];
    for (y, dst) in out.chunks_exact_mut(w).enumerate() {
        let (ya, yc) = (y.saturating_sub(1), (y + 1).min(h - 1));
        let (a, b, c) = (&tmp[ya * w..(ya + 1) * w], &tmp[y * w..(y + 1) * w], &tmp[yc * w..(yc + 1) * w]);
        for (k, o) in dst.iter_mut().enumerate() {
            *o = (a[k] + 2.0 * b[k] + c[k]) * 0.25;
        }
    }
    Gray { w, h, px: out }
}

/// Separable binomial-11 blur (= [1 2 1]/4 then binomial 9: σ² = 2.5 px²),
/// clamped borders.
fn binomial11(g: &Gray) -> Vec<f32> {
    const K: [f32; 11] = [
        1.0 / 1024.0,
        10.0 / 1024.0,
        45.0 / 1024.0,
        120.0 / 1024.0,
        210.0 / 1024.0,
        252.0 / 1024.0,
        210.0 / 1024.0,
        120.0 / 1024.0,
        45.0 / 1024.0,
        10.0 / 1024.0,
        1.0 / 1024.0,
    ];
    let (w, h) = (g.w, g.h);
    let mut tmp = vec![0f32; w * h];
    let mut padded = vec![0f32; w + 10];
    for (src, dst) in g.px.chunks_exact(w).zip(tmp.chunks_exact_mut(w)) {
        padded[5..5 + w].copy_from_slice(src);
        for k in 0..5 {
            padded[k] = src[0];
            padded[w + 5 + k] = src[w - 1];
        }
        for (x, o) in dst.iter_mut().enumerate() {
            let p = &padded[x..x + 11];
            *o = K[0] * (p[0] + p[10])
                + K[1] * (p[1] + p[9])
                + K[2] * (p[2] + p[8])
                + K[3] * (p[3] + p[7])
                + K[4] * (p[4] + p[6])
                + K[5] * p[5];
        }
    }
    let mut out = vec![0f32; w * h];
    let row = |y: isize| {
        let y = y.clamp(0, h as isize - 1) as usize;
        &tmp[y * w..(y + 1) * w]
    };
    for (y, dst) in out.chunks_exact_mut(w).enumerate() {
        let y = y as isize;
        let r: [&[f32]; 11] = std::array::from_fn(|k| row(y + k as isize - 5));
        for (x, o) in dst.iter_mut().enumerate() {
            *o = K[0] * (r[0][x] + r[10][x])
                + K[1] * (r[1][x] + r[9][x])
                + K[2] * (r[2][x] + r[8][x])
                + K[3] * (r[3][x] + r[7][x])
                + K[4] * (r[4][x] + r[6][x])
                + K[5] * r[5][x];
        }
    }
    out
}

#[inline]
fn sobel(p: &[f32], w: usize, i: usize) -> (f32, f32) {
    let a = p[i - w - 1];
    let b = p[i - w];
    let c = p[i - w + 1];
    let d = p[i - 1];
    let f = p[i + 1];
    let g = p[i + w - 1];
    let h = p[i + w];
    let k = p[i + w + 1];
    let gx = (c + 2.0 * f + k - a - 2.0 * d - g) * 0.125;
    let gy = (g + 2.0 * h + k - a - 2.0 * b - c) * 0.125;
    (gx, gy)
}

/// Full edge analysis on a `nx × ny` tile grid. `None` when the image is
/// smaller than `MIN_SIDE`. `classic` also accumulates Tenengrad and the
/// modified Laplacian (benchmarks only).
pub(crate) fn analyze(g: &Gray, nx: usize, ny: usize, classic: bool) -> Option<EdgeAnalysis> {
    analyze_inner(g, nx, ny, classic, None, false)
}

/// `noise`: use these noise levels instead of measuring them on `g`.
/// `noise_only`: stop after measuring the noise.
fn analyze_inner(
    g: &Gray,
    nx: usize,
    ny: usize,
    classic: bool,
    noise: Option<Noise>,
    noise_only: bool,
) -> Option<EdgeAnalysis> {
    let (w, h) = (g.w, g.h);
    if w < MIN_SIDE || h < MIN_SIDE || g.px.len() != w * h {
        return None;
    }
    let (nx, ny) = (nx.clamp(1, w), ny.clamp(1, h));
    let ntiles = nx * ny;
    let tx: Vec<usize> = (0..w).map(|x| x * nx / w).collect();
    let ty: Vec<usize> = (0..h).map(|y| y * ny / h).collect();
    // Column ranges of the tiles, interior pixels only.
    let tcols: Vec<(usize, usize)> = (0..nx)
        .map(|t| {
            let lo = (t * w).div_ceil(nx).max(1);
            let hi = ((t + 1) * w).div_ceil(nx).min(w - 1);
            (lo, hi.max(lo))
        })
        .collect();
    // Fine image (light pre-blur against grain) and re-blurred image
    // (binomial 5 twice = binomial 9, σ0 = √2).
    let fine = binomial3(g);
    let blurred = binomial11(g);
    let src = &g.px;
    let fp = &fine.px;

    let mut tile_lap = vec![0f64; ntiles];
    let mut tile_ten = vec![0f64; ntiles];
    let mut tile_modlap = vec![0f64; ntiles];
    let mut tile_px = vec![0u32; ntiles];
    let mut m2 = vec![0f32; w * h];
    let mut lap2 = vec![0f32; w];
    let mut h_blur = AbsHist::new(0.002, 2048);
    let mut h_grad = AbsHist::new(0.02, 2048);
    let mut h_lap = AbsHist::new(0.05, 2048);
    let n = w - 2;
    for y in 1..h - 1 {
        let (b0, b1, b2) =
            (&blurred[(y - 1) * w..y * w], &blurred[y * w..(y + 1) * w], &blurred[(y + 1) * w..(y + 2) * w]);
        let (s0, s1, s2) = (&src[(y - 1) * w..y * w], &src[y * w..(y + 1) * w], &src[(y + 1) * w..(y + 2) * w]);
        let mrow = &mut m2[y * w + 1..y * w + 1 + n];
        let lrow = &mut lap2[1..1 + n];
        let (b0l, b0c, b0r) = (&b0[..n], &b0[1..n + 1], &b0[2..]);
        let (b1l, b1r) = (&b1[..n], &b1[2..]);
        let (b2l, b2c, b2r) = (&b2[..n], &b2[1..n + 1], &b2[2..]);
        let (s1l, s1c, s1r) = (&s1[..n], &s1[1..n + 1], &s1[2..]);
        let (s0c, s2c) = (&s0[1..n + 1], &s2[1..n + 1]);
        for k in 0..n {
            let gx = (b0r[k] + 2.0 * b1r[k] + b2r[k] - b0l[k] - 2.0 * b1l[k] - b2l[k]) * 0.125;
            let gy = (b2l[k] + 2.0 * b2c[k] + b2r[k] - b0l[k] - 2.0 * b0c[k] - b0r[k]) * 0.125;
            mrow[k] = gx * gx + gy * gy;
            let lap = s1l[k] + s1r[k] + s0c[k] + s2c[k] - 4.0 * s1c[k];
            lrow[k] = lap * lap;
        }
        if noise.is_none() && (y % 4 == 1 || (h < 256 && y % 2 == 1)) {
            // Noise statistics on every 4th row (2nd on small images), every
            // other column.
            for x in (1..w - 1).step_by(2) {
                let i = y * w + x;
                h_blur.add(sobel(&blurred, w, i).0);
                h_grad.add(sobel(fp, w, i).0);
                h_lap.add(lrow[x - 1].sqrt());
            }
        }
        let trow = ty[y] * nx;
        for (t, &(lo, hi)) in tcols.iter().enumerate() {
            tile_lap[trow + t] += lap2[lo..hi].iter().sum::<f32>() as f64;
            tile_px[trow + t] += (hi - lo) as u32;
        }
        if classic {
            for x in 1..w - 1 {
                let i = y * w + x;
                let (gx, gy) = sobel(src, w, i);
                let lxx = (s1[x - 1] + s1[x + 1] - 2.0 * s1[x]).abs();
                let lyy = (s0[x] + s2[x] - 2.0 * s1[x]).abs();
                tile_ten[trow + tx[x]] += (gx * gx + gy * gy) as f64;
                tile_modlap[trow + tx[x]] += (lxx + lyy) as f64;
            }
        }
    }
    let (nb, ng, nl) = match noise {
        Some(n) => (n.blur_grad, n.grad, n.lap),
        None => (h_blur.sigma(), h_grad.sigma(), h_lap.sigma()),
    };
    let noise_g = 2.0 * ng * ng;
    let noise_b = 2.0 * nb * nb;
    let thr = EDGE_ABS_MIN.max(EDGE_NOISE_K * nb);
    let thr2 = thr * thr;
    let lut = width_lut();
    let mut tile_hist = vec![[0u32; NB]; ntiles];
    let mut tile_edges = vec![0u32; ntiles];
    let mut orient_hist = [[0u32; NB]; 4];
    let mut tile_orient = vec![[[0u32; NB]; 4]; ntiles];
    const TAN_22_5: f32 = 0.414_213_57;
    if w > 4 && h > 4 && !noise_only {
        for y in 2..h - 2 {
            let trow = ty[y] * nx;
            let row = &m2[y * w..(y + 1) * w];
            for x in 2..w - 2 {
                let m = row[x];
                if m <= thr2 {
                    continue;
                }
                let i = y * w + x;
                let (bx, by) = sobel(&blurred, w, i);
                let (ax, ay) = (bx.abs(), by.abs());
                let (sector, d) = if ay <= ax * TAN_22_5 {
                    (0usize, 1usize)
                } else if ax <= ay * TAN_22_5 {
                    (2, w)
                } else if (bx > 0.0) == (by > 0.0) {
                    (1, w + 1)
                } else {
                    (3, w - 1)
                };
                if m < m2[i - d] || m <= m2[i + d] {
                    continue;
                }
                let (c0, c1, c2) = (src[i], src[i - 2 * d], src[i + 2 * d]);
                if c0.max(c1).max(c2) >= CLIP_HIGH || c0.min(c1).min(c2) <= CLIP_LOW {
                    continue;
                }
                let (gx, gy) = sobel(fp, w, i);
                let mg = gx * gx + gy * gy - noise_g;
                let mb = m - noise_b;
                if mg <= 0.0 || mb <= 0.0 {
                    continue;
                }
                let bin = width_bin(lut, mg / mb);
                let t = trow + tx[x];
                tile_hist[t][bin] += 1;
                tile_edges[t] += 1;
                orient_hist[sector][bin] += 1;
                tile_orient[t][sector][bin] += 1;
            }
        }
    }
    Some(EdgeAnalysis {
        nx,
        ny,
        tile_hist,
        tile_edges,
        tile_px,
        tile_lap,
        tile_ten,
        tile_modlap,
        orient_hist,
        tile_orient,
        noise_blur_grad: nb,
        noise_grad: ng,
        noise_lap: nl,
    })
}

// ---------------------------------------------------------------------------
// Frame and region focus
// ---------------------------------------------------------------------------

/// Everything the frame-level focus step produces (raw values for
/// `SharpnessMetrics`, and details for calibration).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FrameFocus {
    pub global_energy: f32,
    pub peak_energy: f32,
    /// Edge width of the sharpest region, px at working size.
    pub peak_width: Option<f32>,
    /// Median edge width over the whole frame.
    pub global_width: Option<f32>,
    pub edges: u32,
    pub confidence: f32,
    pub score: f32,
    /// Anisotropy ratio used for motion blur (≥ 1).
    pub motion_ratio: f32,
    pub motion: f32,
    pub noise_sigma: f32,
}

pub(crate) fn grid_for(w: usize, h: usize) -> (usize, usize) {
    let nx = ((w as f32 / TILE_PX as f32).round() as usize).clamp(1, 16);
    let ny = ((h as f32 / TILE_PX as f32).round() as usize).clamp(1, 16);
    (nx, ny)
}

pub(crate) fn frame_focus_from(a: &EdgeAnalysis) -> FrameFocus {
    let ntiles = a.nx * a.ny;
    // Raw energies.
    let mut vols: Vec<f32> = (0..ntiles).map(|t| a.tile_vol(t)).collect();
    vols.sort_by(|x, y| x.total_cmp(y));
    let peak_energy = if vols.is_empty() { 0.0 } else { vols[((vols.len() - 1) as f32 * 0.9).round() as usize] };
    let global_energy = a.global_vol();

    // Peak region edge width.
    let mut valid: Vec<(f32, u32)> = Vec::with_capacity(ntiles);
    for t in 0..ntiles {
        let n = a.tile_edges[t];
        let min_edges = TILE_MIN_EDGES.max(a.tile_px[t] as f32 * TILE_MIN_EDGE_DENSITY);
        if n as f32 >= min_edges {
            if let Some(med) = hist_quantile(&a.tile_hist[t], 0.5) {
                valid.push((med, n));
            }
        }
    }
    valid.sort_by(|x, y| x.0.total_cmp(&y.0));
    let all = a.all_hist();
    let total = a.total_edges();
    let (peak_width, peak_edges) = if !valid.is_empty() {
        let k = PEAK_TILES.min(valid.len());
        let ln_mean = valid[..k].iter().map(|v| v.0.ln()).sum::<f32>() / k as f32;
        let evidence = ((valid.len() as f32 * PEAK_EVIDENCE_FRACTION).ceil() as usize).clamp(k, valid.len());
        let edges: u32 = valid[..evidence].iter().map(|v| v.1).sum();
        (Some(ln_mean.exp()), edges)
    } else if total > 0 {
        // Edges too sparse for any tile to vote: pool them.
        (hist_quantile(&all, 0.25), total)
    } else {
        (None, 0)
    };
    let confidence = peak_edges as f32 / (peak_edges as f32 + FRAME_CONF_EDGES);
    let score = match peak_width {
        Some(wd) => confidence * width_to_score(wd) + (1.0 - confidence) * UNKNOWN_FOCUS,
        None => UNKNOWN_FOCUS,
    };

    // Motion blur, per pair of perpendicular orientations, two estimates of
    // the signed log ratio of edge widths, which must agree:
    // * frame-wide: sharpest quarter of the edges of each orientation;
    // * per tile: same, in tiles holding both orientations, the sharpest
    //   half of them (ranked by their sharper direction, unaffected by motion
    //   along the other one, and large in bokeh) voting with their median.
    // Veto: camera shake leaves no tile sharp in both directions. When some
    // tiles are (`MOTION_VETO_TILES`, at least `MOTION_VETO_SHARE` of the
    // voters, widths ≤ `MOTION_SHARP_BOTH`), the subject is sharp and the
    // anisotropy elsewhere is texture (long hair, fur, a textured wall).
    // Too few edges in one orientation (clipping, plain backgrounds) gives
    // no frame-wide estimate.
    let mut motion = 0f32;
    let mut motion_ratio = 1f32;
    for (sa, sb) in [(0usize, 2usize), (1, 3)] {
        let (na, nb): (u32, u32) = (a.orient_hist[sa].iter().sum(), a.orient_hist[sb].iter().sum());
        if na < MOTION_MIN_EDGES || nb < MOTION_MIN_EDGES {
            continue;
        }
        let (Some(ga), Some(gb)) =
            (hist_quantile(&a.orient_hist[sa], MOTION_QUANTILE), hist_quantile(&a.orient_hist[sb], MOTION_QUANTILE))
        else {
            continue;
        };
        let global = (ga / gb).ln();
        let mut votes: Vec<(f32, f32)> = Vec::new(); // (sharper width, ln ratio)
        let mut sharp_both = 0usize;
        for t in &a.tile_orient {
            let (ta, tb): (u32, u32) = (t[sa].iter().sum(), t[sb].iter().sum());
            if ta < MOTION_TILE_MIN_EDGES || tb < MOTION_TILE_MIN_EDGES {
                continue;
            }
            if let (Some(wa), Some(wb)) =
                (hist_quantile(&t[sa], MOTION_QUANTILE), hist_quantile(&t[sb], MOTION_QUANTILE))
            {
                votes.push((wa.min(wb), (wa / wb).ln()));
                if wa.max(wb) <= MOTION_SHARP_BOTH {
                    sharp_both += 1;
                }
            }
        }
        if votes.is_empty()
            || (sharp_both >= MOTION_VETO_TILES && sharp_both as f32 >= MOTION_VETO_SHARE * votes.len() as f32)
        {
            continue;
        }
        votes.sort_by(|x, y| x.0.total_cmp(&y.0));
        let k = ((votes.len() as f32 * MOTION_TILE_SHARE).ceil() as usize).clamp(1, votes.len());
        let mut lr: Vec<f32> = votes[..k].iter().map(|v| v.1).collect();
        lr.sort_by(|x, y| x.total_cmp(y));
        let tiles = if k % 2 == 1 { lr[k / 2] } else { 0.5 * (lr[k / 2 - 1] + lr[k / 2]) };
        let agreed = if global * tiles > 0.0 { global.abs().min(tiles.abs()) } else { 0.0 };
        let ratio = agreed.exp();
        let conf = k as f32 / (k as f32 + MOTION_CONF_TILES);
        let ramp = ((ratio.ln() - MOTION_RATIO_START.ln()) / (MOTION_RATIO_FULL.ln() - MOTION_RATIO_START.ln()))
            .clamp(0.0, 1.0);
        motion = motion.max(conf * ramp);
        motion_ratio = motion_ratio.max(ratio);
    }

    FrameFocus {
        global_energy,
        peak_energy,
        peak_width,
        global_width: hist_quantile(&all, 0.5),
        edges: total,
        confidence,
        score: score.clamp(0.0, 1.0),
        motion_ratio,
        motion: motion.clamp(0.0, 1.0),
        noise_sigma: a.noise_sigma(),
    }
}

pub(crate) fn frame_focus(g: &Gray) -> FrameFocus {
    let (nx, ny) = grid_for(g.w, g.h);
    match analyze(g, nx, ny, false) {
        Some(a) => frame_focus_from(&a),
        None => FrameFocus { score: UNKNOWN_FOCUS, motion_ratio: 1.0, ..Default::default() },
    }
}

/// Focus of a region (face, eyes): the sharpest `REGION_QUANTILE` of its
/// edges, mapped like the frame score, and never sharper than its blurrier
/// direction (the sharpest tenth of the edges of each orientation, when both
/// orientations of a perpendicular pair are present): a face smeared by
/// motion keeps sharp edges along the motion and must not pass as focused.
/// `noise` comes from a larger context for small regions.
/// Returns (score, width, edges).
pub(crate) fn region_focus(g: &Gray, noise: Option<Noise>) -> (f32, Option<f32>, u32) {
    match analyze_inner(g, 1, 1, false, noise, false) {
        Some(a) => region_focus_of(&a),
        None => (UNKNOWN_FOCUS, None, 0),
    }
}

/// [`region_focus`] of an analysed region.
fn region_focus_of(a: &EdgeAnalysis) -> (f32, Option<f32>, u32) {
    let n = a.total_edges();
    let Some(mut wd) = hist_quantile(&a.tile_hist[0], REGION_QUANTILE) else {
        return (UNKNOWN_FOCUS, None, 0);
    };
    for (sa, sb) in [(0usize, 2usize), (1, 3)] {
        let (ha, hb) = (&a.orient_hist[sa], &a.orient_hist[sb]);
        if ha.iter().sum::<u32>() >= REGION_DIR_MIN_EDGES && hb.iter().sum::<u32>() >= REGION_DIR_MIN_EDGES {
            if let (Some(wa), Some(wb)) =
                (hist_quantile(ha, REGION_DIR_QUANTILE), hist_quantile(hb, REGION_DIR_QUANTILE))
            {
                wd = wd.max(wa.max(wb));
            }
        }
    }
    let conf = n as f32 / (n as f32 + REGION_CONF_EDGES);
    let score = conf * width_to_score(wd) + (1.0 - conf) * UNKNOWN_FOCUS;
    (score.clamp(0.0, 1.0), Some(wd), n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_table_is_monotonic_where_it_matters() {
        let mut prev = f64::INFINITY;
        for s in [0.7, 1.0, 1.5, 2.0, 3.0, 5.0] {
            let r = simulate_ratio(s);
            assert!(r < prev, "ratio must decrease with blur ({s}: {r})");
            assert!(r > 1.0);
            prev = r;
        }
    }

    #[test]
    fn lut_round_trip() {
        let lut = width_lut();
        for s in [0.8f64, 1.2, 2.0, 3.5] {
            let r = simulate_ratio(s) as f32;
            let bin = width_bin(lut, r * r);
            let back = width_at(bin as f64 + 0.5) as f64;
            assert!((back / s - 1.0).abs() < 0.12, "{s} -> {back}");
        }
    }

    #[test]
    fn score_mapping() {
        assert!((width_to_score(S50) - 0.5).abs() < 1e-6);
        assert!(width_to_score(0.4) > 0.95);
        assert!(width_to_score(3.0) < 0.05);
        assert_eq!(width_to_score(f32::NAN), 0.0);
    }
}
