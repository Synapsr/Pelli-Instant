//! Perceptual hashes (64 bits each).
//!
//! * pHash: luma area-resampled to 32×32 (aspect ignored), 2D DCT-II, the 8×8
//!   lowest frequencies *excluding* the DC row and column (pHash.org layout:
//!   coefficients 1..=8 × 1..=8), bit = coefficient > median of the 64.
//!   Robust to resizing, recompression, mild exposure and colour changes.
//! * dHash: luma resampled to 9×8, bit = pixel brighter than its right
//!   neighbour. Cheap second opinion, used together with pHash for exact
//!   duplicates.
//!
//! Bits are packed row-major, most significant bit first.

use crate::prep::{resize_exact, Gray};
use std::sync::OnceLock;

/// Hamming distance between two 64-bit hashes.
pub(crate) fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

fn cos_table() -> &'static [[f32; 32]; 9] {
    static T: OnceLock<[[f32; 32]; 9]> = OnceLock::new();
    T.get_or_init(|| {
        let mut t = [[0f32; 32]; 9];
        for (u, row) in t.iter_mut().enumerate() {
            for (x, v) in row.iter_mut().enumerate() {
                *v = (std::f64::consts::PI * (2 * x + 1) as f64 * u as f64 / 64.0).cos() as f32;
            }
        }
        t
    })
}

pub(crate) fn phash(g: &Gray) -> u64 {
    let small = resize_exact(g, 32, 32);
    if small.is_empty() {
        return 0;
    }
    let c = cos_table();
    // Row transform: t[y][v] = Σ_x f[y][x] c[v][x], v in 1..=8.
    let mut t = [[0f32; 8]; 32];
    for (ty, row) in t.iter_mut().zip(small.px.as_chunks::<32>().0) {
        for (v, out) in ty.iter_mut().enumerate() {
            *out = row.iter().zip(&c[v + 1]).map(|(a, b)| a * b).sum();
        }
    }
    let mut coeffs = [0f32; 64];
    for u in 1..=8 {
        for v in 1..=8 {
            let mut acc = 0f32;
            for (y, ty) in t.iter().enumerate() {
                acc += c[u][y] * ty[v - 1];
            }
            coeffs[(u - 1) * 8 + (v - 1)] = acc;
        }
    }
    // A flat image has only rounding noise in its AC coefficients: hash it
    // to 0 instead of to random bits.
    if coeffs.iter().all(|v| v.abs() < 0.05) {
        return 0;
    }
    let mut sorted = coeffs;
    sorted.sort_by(|a, b| a.total_cmp(b));
    let median = 0.5 * (sorted[31] + sorted[32]);
    pack(coeffs.iter().map(|&v| v > median))
}

pub(crate) fn dhash(g: &Gray) -> u64 {
    let small = resize_exact(g, 9, 8);
    if small.is_empty() {
        return 0;
    }
    pack((0..8).flat_map(|y| {
        let row = &small.px[y * 9..(y + 1) * 9];
        (0..8).map(move |x| row[x] > row[x + 1] + 1e-3)
    }))
}

/// Measurements of two exact copies may differ by at most this much (focus
/// score, mean luma): a re-export or a resized copy keeps them, a blurred or
/// re-exposed frame of the same scene (same hashes!) does not.
pub(crate) const COPY_FOCUS_TOL: f32 = 0.02;
pub(crate) const COPY_LUMA_TOL: f32 = 0.01;

/// What exact-copy detection looks at for one photo.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CopyKey {
    pub phash: u64,
    pub dhash: u64,
    pub focus: f32,
    pub luma: f32,
}

/// Sets (size ≥ 2) of exact copies: identical pHash and dHash (flat frames,
/// hashed to 0, excepted) and the same measurements within tolerance.
pub(crate) fn exact_copy_sets(items: &[CopyKey]) -> Vec<Vec<usize>> {
    let mut buckets: std::collections::HashMap<(u64, u64), Vec<usize>> = Default::default();
    for (i, k) in items.iter().enumerate() {
        if (k.phash, k.dhash) != (0, 0) {
            buckets.entry((k.phash, k.dhash)).or_default().push(i);
        }
    }
    let clean = |v: f32| if v.is_finite() { v } else { -1.0 };
    let mut out = Vec::new();
    let mut buckets: Vec<Vec<usize>> = buckets.into_values().filter(|b| b.len() >= 2).collect();
    buckets.sort();
    for mut b in buckets {
        b.sort_by(|&x, &y| {
            clean(items[x].focus)
                .total_cmp(&clean(items[y].focus))
                .then(clean(items[x].luma).total_cmp(&clean(items[y].luma)))
                .then(x.cmp(&y))
        });
        let close = |x: CopyKey, y: CopyKey| {
            (clean(x.focus) - clean(y.focus)).abs() <= COPY_FOCUS_TOL
                && (clean(x.luma) - clean(y.luma)).abs() <= COPY_LUMA_TOL
        };
        let mut remaining = b;
        while let Some((&anchor, rest)) = remaining.split_first() {
            let (mut set, others): (Vec<usize>, Vec<usize>) =
                rest.iter().partition(|&&i| close(items[i], items[anchor]));
            if !set.is_empty() {
                set.push(anchor);
                set.sort_unstable();
                out.push(set);
            }
            remaining = others;
        }
    }
    out
}

fn pack(bits: impl Iterator<Item = bool>) -> u64 {
    bits.fold(0u64, |acc, b| (acc << 1) | b as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_image_hashes_to_zero() {
        let g = Gray { w: 64, h: 48, px: vec![120.0; 64 * 48] };
        assert_eq!(phash(&g), 0);
        assert_eq!(dhash(&g), 0);
    }

    #[test]
    fn hamming_counts_bits() {
        assert_eq!(hamming(0, u64::MAX), 64);
        assert_eq!(hamming(0b1010, 0b0110), 2);
    }
}
