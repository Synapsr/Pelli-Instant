//! Exposure from the luma histogram.
//!
//! The score estimates **how many stops the frame is off** rather than how far
//! its mean is from mid-grey, so intentional low-key and high-key pictures
//! are not punished:
//!
//! * Underexposure is anchored on the *highlights*: a properly exposed photo,
//!   even a dark moody one, has its brightest 2% (face in the spotlight,
//!   window, candle) near the top of the range. `ev_under` is the number of
//!   stops between the 98th percentile and `HIGHLIGHT_REF`, plus a fraction
//!   (`UNDER_MID_WEIGHT`) of the extra darkness of the midtones. A low-key
//!   portrait (dark background, lit face) gets ~1 stop; a frame shot 3 stops
//!   under gets ~3.
//! * Low-key on a small subject (a black lens on black velvet, a candle): the
//!   brightest 2% may be dark too. When the brightest 0.1% of the frame sits
//!   in the upper tones *without clipping* (`TOP_UNCLIPPED`), the
//!   photographer exposed for those highlights (reflections, a lit edge): the
//!   anchor becomes that 99.9th percentile (`TOP_ALLOWANCE` stops of slack)
//!   and the midtones barely count (`LOWKEY_MID_WEIGHT`). A clipped top is a
//!   light source or a blown area, which says nothing about the subject, so
//!   the 98th percentile stays the anchor. A frame 3 stops under has its top
//!   0.1% below ~0.4 (a highlight at 0.65 would have been at 3× clipping).
//! * Overexposure is anchored on the *midtones and clipping*: the median
//!   brighter than `MID_REF_OVER` (beyond `OVER_MID_FREE` stops) plus the
//!   share of blown pixels beyond `OVER_CLIP_FREE`. A white dress on a white
//!   background keeps a moderate median excess and moderate clipping; a
//!   frame 3 stops over has its median and half the picture at 255.
//!
//! The histogram cannot tell an intentional blown backdrop or sky from lost
//! detail, nor a lit face in a dark room from an underexposed frame. When
//! the photo has a face, `assess` therefore judges the **face skin**
//! ([`skin_ev`]): its luma (65th percentile of the face core, so glasses or
//! a beard do not darken it) and the share of it blown out. A skin luma
//! between `SKIN_LOW` (0.30, deep skin in soft light) and `SKIN_HIGH` (0.80)
//! is fine; beyond, the stops off are counted like the frame's, plus
//! `SKIN_CLIP_EV` stops per unit of blown skin beyond `SKIN_CLIP_FREE`. The
//! frame's own reading still counts, but at most 1 stop beyond the skin's
//! over, 1.5 stops under ([`with_skin`]): a backlit portrait on a white sky
//! (frame +2.5 stops, face fine) reads +1 stop and is kept, a lit face in a
//! dark room reads −1.5 at most; a frame 3 stops under whose fair skin reads
//! 0.24 (skin tone and exposure cannot be told apart from luma alone) still
//! reads −2.1 stops and goes to review.
//!
//! Without a face, a studio portrait on a backdrop blown to pure white over
//! more than half of the frame reads like an overexposed frame. `assess`
//! judges exposure relative to the shoot too, so a whole high-key session is
//! not rejected, and Lenient only rejects near-white frames.
//!
//! Stops are measured in linear light (sRGB transfer function undone).
//! `score = 1 / (1 + exp((ev_error - EV50) / EV_WIDTH))`.

use crate::prep::Gray;

const HIGHLIGHT_REF: f32 = 0.80;
const MID_REF_UNDER: f32 = 0.40;
const UNDER_MID_WEIGHT: f32 = 0.30;
const MID_REF_OVER: f32 = 0.50;
/// 99.9th percentile below which the top of the frame counts as unclipped
/// (full weight below `TOP_UNCLIPPED - TOP_BLEND`, none above it).
const TOP_UNCLIPPED: f32 = 0.995;
const TOP_BLEND: f32 = 0.03;
const TOP_ALLOWANCE: f32 = 0.5;
const LOWKEY_MID_WEIGHT: f32 = 0.10;
const OVER_MID_FREE: f32 = 0.50;
const OVER_CLIP_FREE: f32 = 0.12;
/// Stops of penalty per unit fraction of blown pixels beyond the free share.
const OVER_CLIP_EV: f32 = 2.5;
/// Stops off at which the score is 0.5, and logistic width.
pub(crate) const EV50: f32 = 2.0;
const EV_WIDTH: f32 = 0.35;
/// Face skin: median luma range read as well exposed, stops per unit of
/// blown skin beyond the free share, and how far beyond the skin's reading
/// the frame's may go.
const SKIN_LOW: f32 = 0.30;
const SKIN_HIGH: f32 = 0.80;
const SKIN_CLIP_FREE: f32 = 0.10;
const SKIN_CLIP_EV: f32 = 3.0;
const SKIN_FRAME_OVER: f32 = 1.0;
const SKIN_FRAME_UNDER: f32 = 1.5;

/// Exposure score of `ev_error` stops off.
pub(crate) fn score_of_ev(err: f32) -> f32 {
    (1.0 / (1.0 + ((err - EV50) / EV_WIDTH).exp())).clamp(0.0, 1.0)
}

/// Stops off of an exposure score (inverse of [`score_of_ev`]).
pub(crate) fn ev_of_score(score: f32) -> f32 {
    let s = score.clamp(1e-4, 1.0 - 1e-4);
    EV50 + EV_WIDTH * (1.0 / s - 1.0).ln()
}

/// Stops (under, over) of a face from its skin luma and blown share.
pub(crate) fn skin_ev(median: f32, clipped: f32) -> (f32, f32) {
    let under = stops(SKIN_LOW, median.max(0.005)).max(0.0);
    let over = stops(median, SKIN_HIGH).max(0.0) + SKIN_CLIP_EV * (clipped - SKIN_CLIP_FREE).max(0.0);
    (under, over)
}

/// Exposure of a photo with a face: (score, bright). The frame's stops off
/// (`frame_score`, over when `frame_bright`) count at most
/// `SKIN_FRAME_OVER` / `SKIN_FRAME_UNDER` stops beyond the skin's.
pub(crate) fn with_skin(frame_score: f32, frame_bright: bool, median: f32, clipped: f32) -> (f32, bool) {
    let (su, so) = skin_ev(median, clipped);
    let frame = ev_of_score(frame_score).max(0.0);
    let (fu, fo) = if frame_bright { (0.0, frame) } else { (frame, 0.0) };
    let under = su.max(fu.min(su + SKIN_FRAME_UNDER));
    let over = so.max(fo.min(so + SKIN_FRAME_OVER));
    let bright = if over == under { frame_bright } else { over > under };
    (score_of_ev(under.max(over)), bright)
}

/// How far beyond the subject region's own overexposure the frame's may
/// count (stops). Less than for a face (`SKIN_FRAME_OVER`): a figure lit
/// from behind is itself bright (+1.8 stops for a back-lit bride whose sky
/// reads +2.6).
const SUBJECT_FRAME_OVER: f32 = 0.5;

/// Exposure of a photo without a usable face but with a subject region
/// (`SubjectObservation`): its overexposure is the subject's own, the frame
/// counting at most `SUBJECT_FRAME_OVER` stop beyond it; the underexposure
/// stays the frame's (clothes and fur say nothing about it: a dark coat is
/// not a dark photo).
pub(crate) fn with_subject(frame_score: f32, frame_bright: bool, subject_over: f32) -> f32 {
    if !frame_bright || !subject_over.is_finite() {
        return frame_score;
    }
    let frame = ev_of_score(frame_score).max(0.0);
    score_of_ev(frame.min(subject_over.max(0.0) + SUBJECT_FRAME_OVER))
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ExposureStats {
    pub mean: f32,
    pub median: f32,
    pub p02: f32,
    pub p98: f32,
    pub p999: f32,
    pub clip_hi: f32,
    pub clip_lo: f32,
    pub contrast: f32,
    pub ev_under: f32,
    pub ev_over: f32,
    pub score: f32,
}

fn srgb_to_linear(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn stops(a: f32, b: f32) -> f32 {
    (srgb_to_linear(a).max(1e-6) / srgb_to_linear(b).max(1e-6)).log2()
}

fn quantile(hist: &[u32; 256], n: u64, q: f32) -> f32 {
    let target = (q.clamp(0.0, 1.0) as f64 * n as f64).max(0.5);
    let mut cum = 0f64;
    for (k, &c) in hist.iter().enumerate() {
        let c = c as f64;
        if c > 0.0 && cum + c >= target {
            // Linear interpolation inside the bin [k - 0.5, k + 0.5].
            let frac = ((target - cum) / c) as f32;
            return ((k as f32 - 0.5 + frac) / 255.0).clamp(0.0, 1.0);
        }
        cum += c;
    }
    1.0
}

pub(crate) fn exposure(g: &Gray) -> ExposureStats {
    if g.is_empty() {
        return ExposureStats::default();
    }
    // Four interleaved histograms break the store-to-load dependency of
    // consecutive equal pixels. `as` saturates (NaN → 0): no branch needed.
    let mut sub = [[0u32; 256]; 4];
    let bin = |v: f32| ((v + 0.5) as u32).min(255) as usize;
    let (chunks, rest) = g.px.as_chunks::<4>();
    for &v in rest {
        sub[0][bin(v)] += 1;
    }
    for c in chunks {
        sub[0][bin(c[0])] += 1;
        sub[1][bin(c[1])] += 1;
        sub[2][bin(c[2])] += 1;
        sub[3][bin(c[3])] += 1;
    }
    let mut hist = [0u32; 256];
    for (k, h) in hist.iter_mut().enumerate() {
        *h = sub[0][k] + sub[1][k] + sub[2][k] + sub[3][k];
    }
    let n = g.px.len() as u64;
    // Mean and RMS contrast from the histogram (1-level quantisation).
    let (mut sum, mut sum2) = (0f64, 0f64);
    for (k, &c) in hist.iter().enumerate() {
        let u = k as f64 / 255.0;
        sum += c as f64 * u;
        sum2 += c as f64 * u * u;
    }
    let mean = (sum / n as f64) as f32;
    let var = (sum2 / n as f64 - (sum / n as f64).powi(2)).max(0.0);
    let clip_hi = hist[250..].iter().map(|&c| c as u64).sum::<u64>() as f32 / n as f32;
    let clip_lo = hist[..=5].iter().map(|&c| c as u64).sum::<u64>() as f32 / n as f32;
    let median = quantile(&hist, n, 0.5);
    let p02 = quantile(&hist, n, 0.02);
    let p98 = quantile(&hist, n, 0.98);
    let p999 = quantile(&hist, n, 0.999);

    let hl98 = stops(HIGHLIGHT_REF, p98.max(0.02)).clamp(0.0, 8.0);
    let hl_top = stops(HIGHLIGHT_REF, p999.max(0.02)).clamp(0.0, 8.0);
    // Weight of the "exposed for the unclipped highlights" reading.
    let c = ((TOP_UNCLIPPED - p999) / TOP_BLEND).clamp(0.0, 1.0);
    let hl = hl98 + c * ((hl_top + TOP_ALLOWANCE).min(hl98) - hl98);
    let lowkey_weight = LOWKEY_MID_WEIGHT + (UNDER_MID_WEIGHT - LOWKEY_MID_WEIGHT) * (hl_top - 0.5).clamp(0.0, 1.0);
    let mid_weight = UNDER_MID_WEIGHT + c * (lowkey_weight - UNDER_MID_WEIGHT);
    let md = stops(MID_REF_UNDER, median.max(0.01)).clamp(0.0, 10.0);
    let ev_under = hl + mid_weight * (md - hl).max(0.0);
    let md_over = (stops(median, MID_REF_OVER) - OVER_MID_FREE).max(0.0);
    let ev_over = md_over + OVER_CLIP_EV * (clip_hi - OVER_CLIP_FREE).max(0.0);
    let score = score_of_ev(ev_under.max(ev_over));

    ExposureStats {
        mean,
        median,
        p02,
        p98,
        p999,
        clip_hi,
        clip_lo,
        contrast: var.sqrt() as f32,
        ev_under,
        ev_over,
        score: score.clamp(0.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gray_ramp(lo: f32, hi: f32) -> Gray {
        let (w, h) = (256, 64);
        let px = (0..w * h).map(|i| lo + (hi - lo) * (i % w) as f32 / (w - 1) as f32).collect();
        Gray { w, h, px }
    }

    #[test]
    fn full_ramp_is_well_exposed() {
        let e = exposure(&gray_ramp(0.0, 255.0));
        assert!((e.mean - 0.5).abs() < 0.01);
        assert!((e.median - 0.5).abs() < 0.01);
        assert!(e.score > 0.9, "{e:?}");
        assert!(e.clip_hi > 0.01 && e.clip_hi < 0.03);
    }

    #[test]
    fn black_frame_is_underexposed() {
        let e = exposure(&gray_ramp(0.0, 12.0));
        assert!(e.score < 0.05, "{e:?}");
        assert!(e.shadows_heavy());
    }

    #[test]
    fn white_frame_is_overexposed() {
        let e = exposure(&gray_ramp(250.0, 255.0));
        assert!(e.score < 0.05, "{e:?}");
    }

    #[test]
    fn ev_and_score_round_trip() {
        for ev in [0.5f32, 1.5, 2.0, 2.7] {
            assert!((ev_of_score(score_of_ev(ev)) - ev).abs() < 1e-3, "{ev}");
        }
    }

    #[test]
    fn the_face_skin_decides_on_portraits() {
        // Backlit portrait: sky blown (frame 2.6 stops over), face fine.
        let frame = score_of_ev(2.6);
        assert!(frame < 0.2);
        let (s, bright) = with_skin(frame, true, 0.62, 0.0);
        assert!(s > 0.9 && bright, "{s}");
        // Low-key portrait: dark room, lit face (1.5 stops under at most).
        let (s, _) = with_skin(score_of_ev(2.2), false, 0.55, 0.0);
        assert!(s > 0.75, "{s}");
        // Half the skin blown (to review), all of it (rejected).
        let (s, bright) = with_skin(score_of_ev(0.5), true, 0.98, 0.6);
        assert!(s < 0.4 && bright, "{s}");
        assert!(with_skin(score_of_ev(0.5), true, 1.0, 1.0).0 < 0.02);
        // Silhouette: black face on a fine sky.
        let (s, bright) = with_skin(score_of_ev(0.6), true, 0.04, 0.0);
        assert!(s < 0.1 && !bright, "{s}");
        // Whole frame 3 stops under: fair skin reads 0.24, to review.
        let (s, bright) = with_skin(score_of_ev(3.0), false, 0.24, 0.0);
        assert!(s < 0.5 && !bright, "{s}");
        // Deep skin, well exposed in soft light.
        assert!(with_skin(score_of_ev(1.2), false, 0.27, 0.0).0 > 0.9);
    }

    impl ExposureStats {
        fn shadows_heavy(&self) -> bool {
            self.clip_lo > 0.3
        }
    }
}
