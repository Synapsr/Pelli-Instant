//! Per-photo measures on synthetic images: focus vs blur, motion blur,
//! shallow depth of field, noise, low texture, exposure, hashes, regions and
//! degenerate inputs.

mod common;

use common::*;
use cull_analysis::internals::{focus_debug, hamming};
use cull_analysis::{body_hash, compute_metrics, eye_sharpness, region_sharpness, skin_exposure, UNKNOWN_FOCUS};
use cull_types::{FaceObservation, NormPoint, NormRect};
use image::{Rgb, RgbImage};

const W: u32 = 1200;
const H: u32 = 800;

fn focus(img: &RgbImage) -> f32 {
    compute_metrics(img).sharpness.score
}

#[test]
fn focus_score_decreases_monotonically_with_blur() {
    for seed in [1, 2] {
        let sharp = scene(W, H, seed);
        // σ in preview pixels (1200 px → 1024 px working copy).
        let scores: Vec<f32> = [0.0, 0.8, 1.5, 2.2, 3.0, 4.5]
            .iter()
            .map(|&s| if s == 0.0 { focus(&sharp) } else { focus(&gaussian_blur(&sharp, s)) })
            .collect();
        for w in scores.windows(2) {
            assert!(w[1] < w[0], "seed {seed}: not decreasing {scores:?}");
        }
        assert!(scores[0] > 0.9, "sharp scene {scores:?}");
        assert!(scores[3] < 0.3, "σ 2.2 must be blurry {scores:?}");
        assert!(scores[5] < 0.1, "σ 4.5 must be very blurry {scores:?}");
    }
}

#[test]
fn edge_width_reads_the_blur_radius() {
    let sharp = scene(1024, 683, 3);
    for sigma in [1.0f32, 2.0, 3.0] {
        let w = focus_debug(&gaussian_blur(&sharp, sigma)).peak_width.unwrap();
        assert!((w / sigma - 1.0).abs() < 0.2, "σ {sigma} read as {w}");
    }
}

#[test]
fn motion_blur_is_directional() {
    let sharp = scene(W, H, 4);
    let base = compute_metrics(&sharp).sharpness.motion_blur;
    assert!(base < 0.2, "sharp: {base}");
    for angle in [0.0, 30.0, 90.0, 135.0] {
        let m = compute_metrics(&motion_blur(&sharp, 12.0, angle)).sharpness.motion_blur;
        assert!(m > 0.6, "motion at {angle}°: {m}");
    }
    let defocus = compute_metrics(&gaussian_blur(&sharp, 2.5)).sharpness.motion_blur;
    assert!(defocus < 0.2, "isotropic blur is not motion: {defocus}");
}

#[test]
fn oriented_texture_is_not_motion_blur() {
    // Long straight hair strands over a soft textured wall, and a sharp face
    // in the middle (portrait_07-like): no motion blur.
    let mut rng = Rng::new(31);
    let strands: Vec<(f32, f32, f32)> =
        (0..140).map(|_| (rng.range(0.0, W as f32), rng.range(40.0, 120.0), rng.range(1.0, 2.5))).collect();
    let base = gaussian_blur(&add_noise(&RgbImage::from_pixel(W, H, Rgb([190, 40, 50])), 18.0, 5), 2.0);
    let face = scene(300, 300, 32);
    let img = RgbImage::from_fn(W, H, |x, y| {
        let (fx, fy) = (x as i64 - (W as i64 / 2 - 150), y as i64 - (H as i64 / 2 - 150));
        if (0..300).contains(&fx) && (0..300).contains(&fy) {
            return *face.get_pixel(fx as u32, fy as u32);
        }
        for &(sx, tone, width) in &strands {
            let xf = x as f32;
            if (xf - sx).abs() < width && (xf < 0.35 * W as f32 || xf > 0.65 * W as f32) {
                return Rgb([tone as u8 + 40, tone as u8 + 20, tone as u8]);
            }
        }
        *base.get_pixel(x, y)
    });
    let m = compute_metrics(&img).sharpness;
    assert!(m.motion_blur < 0.3, "hair is not motion: {m:?}");
    // Real camera shake is still seen (the only tiles holding both
    // orientations are the face's, hence a capped confidence).
    for angle in [0.0, 90.0, 45.0] {
        let dbg = focus_debug(&motion_blur(&img, 10.0, angle));
        assert!(dbg.motion >= 0.45 && dbg.motion_ratio > 2.0, "camera shake {angle}°: {dbg:?}");
    }
}

#[test]
fn shallow_depth_of_field_subject_is_sharp() {
    let sharp = scene(W, H, 5);
    let portrait = shallow_dof(&sharp, 0.45, 5.0);
    let all_blurred = gaussian_blur(&sharp, 5.0);
    let m = compute_metrics(&portrait).sharpness;
    assert!(m.score > 0.85, "sharp subject on bokeh must not be blurry: {m:?}");
    assert!(m.peak > m.global, "peak region sharper than the frame: {m:?}");
    assert!(focus(&all_blurred) < 0.1);
}

#[test]
fn noise_neither_hides_blur_nor_fakes_sharpness() {
    let sharp = scene(W, H, 6);
    let blurred = gaussian_blur(&sharp, 3.0);
    for sigma in [6.0, 12.0] {
        let s = focus(&add_noise(&sharp, sigma, 1));
        let b = focus(&add_noise(&blurred, sigma, 2));
        assert!(s > 0.85, "noisy sharp σn {sigma}: {s}");
        assert!(b < 0.3, "noisy blurred σn {sigma}: {b}");
    }
}

#[test]
fn low_texture_is_not_called_blurry() {
    let sky = smooth_sky(W, H, 7);
    let s = focus(&sky);
    assert!((s - UNKNOWN_FOCUS).abs() < 0.15, "plain sky: {s}");
    let flat = RgbImage::from_pixel(W, H, Rgb([200, 200, 200]));
    assert!((focus(&flat) - UNKNOWN_FOCUS).abs() < 1e-6);
}

#[test]
fn exposure_follows_stops_and_tolerates_low_and_high_key() {
    let img = scene(W, H, 8);
    let score = |ev: f32| compute_metrics(&shift_ev(&img, ev)).exposure.score;
    let (m3, m2, m1, z) = (score(-3.0), score(-2.0), score(-1.0), score(0.0));
    let (p1, p3, p4) = (score(1.0), score(3.0), score(4.0));
    assert!(z > 0.9 && m1 > 0.8 && p1 > 0.8, "{z} {m1} {p1}");
    assert!(m3 < m2 && m2 < m1, "{m3} {m2} {m1}");
    assert!(m3 < 0.2, "3 stops under: {m3}");
    // Overexposure is judged on blown highlights and midtones only (a dark
    // scene pushed 3 stops can look normal), so it bites later.
    assert!(p3 < 0.7 * z && p4 < 0.2, "over: +3 {p3}, +4 {p4}");

    // Low key: dark background, lit subject.
    let low_key = RgbImage::from_fn(W, H, |x, y| {
        let (dx, dy) = (x as f32 / W as f32 - 0.5, y as f32 / H as f32 - 0.45);
        let v = if dx * dx + dy * dy < 0.035 { 150.0 + 80.0 * ((x * 7 + y * 3) % 13) as f32 / 13.0 } else { 14.0 };
        Rgb([v as u8, v as u8, v as u8])
    });
    let lk = compute_metrics(&low_key).exposure;
    assert!(lk.score > 0.6, "low key is fine: {lk:?}");
    assert!(compute_metrics(&shift_ev(&low_key, -3.0)).exposure.score < 0.2);

    // Low-key product shot: a black object on black velvet, lit so that a few
    // reflections (0.2% of the frame) sit high but unclipped.
    let product = RgbImage::from_fn(W, H, |x, y| {
        let (u, v) = (x as f32 / W as f32, y as f32 / H as f32);
        let body = (u - 0.5).abs() < 0.18 && (v - 0.55).abs() < 0.15;
        let rim = body && (v - 0.42).abs() < 0.004 && (u - 0.5).abs() < 0.17;
        let px = if rim {
            170.0 + (x % 7) as f32 * 2.0
        } else if body {
            22.0 + ((x * 3 + y * 5) % 23) as f32
        } else {
            6.0 + ((x * 7 + y * 13) % 11) as f32
        };
        Rgb([px as u8; 3])
    });
    let pr = compute_metrics(&product).exposure;
    assert!(pr.score > 0.5, "intentional low key with exposed highlights: {pr:?}");
    let pr3 = compute_metrics(&shift_ev(&product, -3.0)).exposure;
    assert!(pr3.score < 0.1, "the same shot 3 stops under: {pr3:?}");

    // High key: a white dress on a white background — bright, a third of
    // the frame clipped, detail kept in the folds, a face and dark hair.
    let high_key = RgbImage::from_fn(W, H, |x, y| {
        let (u, v) = (x as f32 / W as f32, y as f32 / H as f32);
        let px = if (u - 0.5).abs() < 0.08 && v < 0.3 {
            60.0 + 120.0 * ((x * 5 + y * 11) % 17) as f32 / 17.0 // face and hair
        } else if (u - 0.5).abs() < 0.25 && v >= 0.3 {
            215.0 + 30.0 * ((x * 3 + y * 7) % 11) as f32 / 11.0 // dress folds
        } else if u < 0.3 {
            252.0 // blown part of the backdrop
        } else {
            232.0 + 15.0 * v // backdrop with detail
        };
        Rgb([px.min(255.0) as u8; 3])
    });
    let hk = compute_metrics(&high_key).exposure;
    assert!(hk.score > 0.6, "high key is fine: {hk:?}");
    assert!(hk.median_luma > 0.85 && hk.highlights_clipped > 0.2, "{hk:?}");
    // The same frame 2 stops over is blown.
    let over = compute_metrics(&shift_ev(&high_key, 2.0)).exposure;
    assert!(over.score < 0.1, "{over:?}");
}

#[test]
fn perceptual_hashes_survive_resize_and_recompression() {
    let a = scene(W, H, 9);
    let ha = compute_metrics(&a).hashes;
    let variants = [
        resize(&a, W / 2, H / 2),
        resize(&a, 3 * W / 2, 3 * H / 2),
        jpeg_roundtrip(&a, 60),
        shift_ev(&a, 0.3),
        add_noise(&a, 4.0, 3),
    ];
    for (k, v) in variants.iter().enumerate() {
        let hv = compute_metrics(v).hashes;
        assert!(hamming(ha.phash, hv.phash) <= 6, "variant {k}: pHash distance {}", hamming(ha.phash, hv.phash));
        assert!(hamming(ha.dhash, hv.dhash) <= 10, "variant {k}: dHash distance {}", hamming(ha.dhash, hv.dhash));
    }
    // Slight reframing (2% each side) stays near the grouping distance (≤ 10;
    // on real photos the median is 6, max 12).
    let crop = resize(&common::crop(&a, W / 50, H / 50, W * 48 / 50, H * 48 / 50), W, H);
    assert!(hamming(ha.phash, compute_metrics(&crop).hashes.phash) <= 12);
    for seed in 10..16 {
        let hb = compute_metrics(&scene(W, H, seed)).hashes;
        assert!(hamming(ha.phash, hb.phash) >= 16, "different scenes too close: {}", hamming(ha.phash, hb.phash));
    }
}

#[test]
fn region_sharpness_tracks_focus_and_handles_small_regions() {
    let sharp = scene(2400, 1600, 11);
    let face = NormRect { x: 0.4, y: 0.35, w: 0.12, h: 0.18 };
    let s0 = region_sharpness(&sharp, face);
    let s1 = region_sharpness(&gaussian_blur(&sharp, 2.0), face);
    let s2 = region_sharpness(&gaussian_blur(&sharp, 5.0), face);
    assert!(s0 > 0.85 && s1 < s0 && s2 < s1 && s2 < 0.2, "{s0} {s1} {s2}");

    // Same scale as the frame score at the working resolution.
    let work = resize(&sharp, 1024, 683);
    let whole = NormRect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
    assert!((region_sharpness(&work, whole) - focus(&work)).abs() < 0.1);

    // Eyes-nose box of a small face (~40 px, what the engine measures on a
    // 60 px face): sharp reads sharp, not "unknown".
    let blurred3 = gaussian_blur(&sharp, 3.0);
    let mut measured = 0;
    for k in 0..25 {
        let (x, y) = (0.1 + 0.16 * (k % 5) as f32, 0.1 + 0.16 * (k / 5) as f32);
        let small_face = NormRect { x, y, w: 40.0 / 2400.0, h: 40.0 / 1600.0 };
        let f_sharp = region_sharpness(&sharp, small_face);
        if (f_sharp - UNKNOWN_FOCUS).abs() < 0.02 {
            continue; // flat patch: nothing to measure
        }
        measured += 1;
        // Blurred: low, or no edge left at all (unknown, which never vouches
        // for the subject: the sharp-face override needs ≥ 0.75).
        let f_blur = region_sharpness(&blurred3, small_face);
        assert!(
            f_sharp > 0.75 && f_blur <= UNKNOWN_FOCUS + 0.01 && f_blur < f_sharp - 0.2,
            "({x}, {y}): {f_sharp} {f_blur}"
        );
    }
    assert!(measured >= 5, "{measured} small patches measured");

    // Tiny region: enlarged to MIN_REGION_PX, still meaningful.
    let tiny = NormRect { x: 0.5, y: 0.5, w: 0.002, h: 0.002 };
    let t_sharp = region_sharpness(&sharp, tiny);
    let t_blur = region_sharpness(&gaussian_blur(&sharp, 5.0), tiny);
    assert!((0.0..=1.0).contains(&t_sharp) && t_blur <= t_sharp, "{t_sharp} {t_blur}");

    // Degenerate inputs.
    for r in [
        NormRect { x: f32::NAN, y: 0.2, w: 0.1, h: 0.1 },
        NormRect { x: 1.5, y: 1.5, w: 0.1, h: 0.1 },
        NormRect { x: 0.2, y: 0.2, w: -0.1, h: -0.1 },
        NormRect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 },
    ] {
        let v = region_sharpness(&sharp, r);
        assert!((0.0..=1.0).contains(&v), "{r:?} → {v}");
    }
    assert_eq!(region_sharpness(&RgbImage::new(0, 0), face), UNKNOWN_FOCUS);
    assert_eq!(region_sharpness(&RgbImage::new(1, 1), face), UNKNOWN_FOCUS);
}

#[test]
fn degenerate_images_never_panic() {
    let sizes = [(0, 0), (1, 1), (3, 2), (7, 9), (10_000, 3), (3, 10_000), (4097, 17), (64, 64)];
    for (w, h) in sizes {
        for img in [
            RgbImage::from_pixel(w, h, Rgb([0, 0, 0])),
            RgbImage::from_pixel(w, h, Rgb([255, 255, 255])),
            RgbImage::from_fn(w, h, |x, y| Rgb([(x * 37 % 256) as u8, (y * 91 % 256) as u8, ((x + y) % 256) as u8])),
        ] {
            let m = compute_metrics(&img);
            let s = m.sharpness;
            let e = m.exposure;
            for v in [s.score, s.motion_blur, e.score, e.mean_luma, e.median_luma, e.contrast] {
                assert!(v.is_finite() && (0.0..=1.0).contains(&v), "{w}×{h}: {m:?}");
            }
            assert!(s.global.is_finite() && s.peak.is_finite());
        }
    }
    let black = compute_metrics(&RgbImage::from_pixel(640, 480, Rgb([0, 0, 0])));
    assert!(black.exposure.score < 0.05 && black.exposure.shadows_clipped > 0.99);
    let white = compute_metrics(&RgbImage::from_pixel(640, 480, Rgb([255, 255, 255])));
    assert!(white.exposure.score < 0.05 && white.exposure.highlights_clipped > 0.99);
}

#[test]
fn metrics_are_deterministic_and_resolution_independent() {
    let img = scene(W, H, 12);
    assert_eq!(compute_metrics(&img), compute_metrics(&img));
    // Same photo from a 1.5× larger preview: same verdict-level values.
    let big = resize(&img, W * 2, H * 2);
    let (a, b) = (compute_metrics(&img), compute_metrics(&big));
    assert!((a.sharpness.score - b.sharpness.score).abs() < 0.1, "{a:?} {b:?}");
    assert!((a.exposure.score - b.exposure.score).abs() < 0.05);
    assert!(hamming(a.hashes.phash, b.hashes.phash) <= 4);
}

/// A face box with its eyes at `(0.4, 0.45)` and `(0.6, 0.45)` of the image.
fn face_at(bbox: NormRect) -> FaceObservation {
    let p = |x: f32, y: f32| NormPoint { x, y };
    FaceObservation {
        bbox,
        confidence: 0.95,
        landmarks: [p(0.4, 0.45), p(0.6, 0.45), p(0.5, 0.55), p(0.42, 0.65), p(0.58, 0.65)],
        right_eye_open: None,
        left_eye_open: None,
        sharpness: None,
        area_ratio: bbox.w * bbox.h,
        mesh_presence: Some(1.0),
        pose: None,
        expression: None,
        skin_luma: None,
        skin_clipped: None,
        body_hash: None,
    }
}

#[test]
fn a_small_sharp_zone_on_bokeh_is_sharp() {
    // Product detail at f/2: a sharp zone of ~4% of the frame, the rest in
    // deep bokeh. Only when nothing is sharp is the frame blurry.
    let sharp = scene(W, H, 41);
    let bokeh = gaussian_blur(&sharp, 5.0);
    let (zx, zy, zw, zh) = (500, 330, 220, 170);
    let detail = RgbImage::from_fn(W, H, |x, y| {
        let inside = (zx..zx + zw).contains(&x) && (zy..zy + zh).contains(&y);
        *if inside { sharp.get_pixel(x, y) } else { bokeh.get_pixel(x, y) }
    });
    assert!(focus(&detail) > 0.7, "small sharp zone: {}", focus(&detail));
    assert!(focus(&bokeh) < 0.15, "nothing sharp: {}", focus(&bokeh));
}

#[test]
fn eye_focus_reads_the_same_at_any_size_and_tracks_blur() {
    // A face in focus through a real lens (σ 2 px at 1600 px), seen in a
    // 1600 px and an 800 px preview: 320 and 160 px between the eyes.
    let big = gaussian_blur(&scene(1600, 1067, 43), 2.0);
    let bbox = NormRect { x: 0.3, y: 0.3, w: 0.4, h: 0.5 };
    let f = face_at(bbox);
    let at_1600 = eye_sharpness(&big, &f).unwrap();
    let small = resize(&big, 800, 533);
    let at_800 = eye_sharpness(&small, &f).unwrap();
    // Both measured at 64 px between the eyes.
    assert!(at_1600 > 0.8 && (at_1600 - at_800).abs() < 0.1, "{at_1600} vs {at_800}");
    // The face region measured at native size does not: the same face
    // reads far softer the larger it is in pixels.
    let region = NormRect { x: bbox.x + 0.15 * bbox.w, y: bbox.y + 0.15 * bbox.h, w: 0.7 * bbox.w, h: 0.55 * bbox.h };
    let (r1600, r800) = (region_sharpness(&big, region), region_sharpness(&small, region));
    assert!(r1600 < r800 - 0.2, "{r1600} vs {r800}");
    // Missed focus (σ 8 px at 1600 px, 1.6 px between eyes 64 px apart).
    let soft = eye_sharpness(&gaussian_blur(&big, 8.0), &f).unwrap();
    assert!(soft < 0.4, "{soft}");
    // Camera shake across the lids. (Along them, the lids stay sharp: the
    // frame-level motion measure judges that case.)
    let shaken = eye_sharpness(&motion_blur(&big, 40.0, 90.0), &f).unwrap();
    assert!(shaken < at_1600 - 0.2, "{shaken}");
    // Too small to place the eyes, or no edge at all: no reading.
    assert!(eye_sharpness(&resize(&big, 30, 20), &f).is_none());
    assert!(eye_sharpness(&RgbImage::from_pixel(800, 533, Rgb([90, 90, 90])), &f).is_none());
}

#[test]
fn skin_exposure_reads_the_face_core_whatever_the_glasses() {
    // Grey room, a face of luma ~180, sunglasses over a quarter of its core.
    let bbox = NormRect { x: 0.4, y: 0.2, w: 0.2, h: 0.3 };
    let (w, h) = (1000u32, 800u32);
    let img = RgbImage::from_fn(w, h, |x, y| {
        let (u, v) = ((x as f32 / w as f32 - bbox.x) / bbox.w, (y as f32 / h as f32 - bbox.y) / bbox.h);
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            Rgb([60, 60, 60])
        } else if (0.3..0.45).contains(&v) {
            Rgb([10, 10, 10]) // sunglasses
        } else {
            Rgb([200, 175, 160])
        }
    });
    let (luma, clipped) = skin_exposure(&img, &face_at(bbox)).unwrap();
    assert!((luma - 0.71).abs() < 0.03 && clipped == 0.0, "{luma} {clipped}");
    // A blown face.
    let white = RgbImage::from_fn(w, h, |x, _| if x > 300 { Rgb([255, 255, 255]) } else { Rgb([60, 60, 60]) });
    let (luma, clipped) = skin_exposure(&white, &face_at(bbox)).unwrap();
    assert!(luma > 0.99 && clipped > 0.99);
}

#[test]
fn body_hash_follows_the_subject_and_sees_the_arms() {
    // A person on a soft background: face, torso, arms down or raised.
    let wall = gaussian_blur(&scene(1400, 900, 47), 15.0);
    let person = |dx: i32, arm_up: bool| {
        RgbImage::from_fn(1200, 800, |x, y| {
            let (x, y) = (x as i32 - dx, y as i32);
            let face = (x - 600).pow(2) + (y - 200).pow(2) < 45 * 45;
            let torso = (530..670).contains(&x) && (250..600).contains(&y);
            // Both hands in the hair, or both arms down along the body.
            let side = (x - 600).abs();
            let arm = if arm_up {
                (50..130).contains(&side) && (90..300).contains(&y)
            } else {
                (70..150).contains(&side) && (260..560).contains(&y)
            };
            if face {
                Rgb([210, 170, 150])
            } else if torso || arm {
                Rgb([40, 60, 120])
            } else {
                *wall.get_pixel((x + 100).clamp(0, 1399) as u32, y.clamp(0, 899) as u32)
            }
        })
    };
    let face = |dx: i32| {
        let b = NormRect { x: (555 + dx) as f32 / 1200.0, y: 155.0 / 800.0, w: 90.0 / 1200.0, h: 90.0 / 800.0 };
        FaceObservation { area_ratio: b.w * b.h, bbox: b, ..face_at(b) }
    };
    let down = body_hash(&person(0, false), &face(0)).unwrap();
    let moved = body_hash(&person(-120, false), &face(-120)).unwrap();
    let up = body_hash(&person(0, true), &face(0)).unwrap();
    assert!(hamming(down, moved) <= 8, "the crop follows the face: {}", hamming(down, moved));
    assert!(hamming(down, up) >= 10, "a raised arm changes it: {}", hamming(down, up));
}
