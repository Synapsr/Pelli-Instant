//! Calibration table and benchmarks for cull-analysis.
//!
//! ```sh
//! scripts/fetch-calibration-photos.sh            # once, into samples/photos
//! cargo run --release -p cull-analysis --example calibrate [photos_dir] [--verbose]
//! ```
//!
//! Prints:
//! 1. focus/exposure on synthetic scenes and their degraded variants;
//! 2. for every real photo: the original and generated variants (Gaussian
//!    blur, motion blur, noise, ±EV, near-duplicate crop, recompression) with
//!    focus score, edge width, motion, exposure score, stops off, verdict and
//!    reasons (`--verbose` for every row, summary otherwise);
//! 3. the separation (AUC) of sharp vs blurred photos for the edge-width
//!    score and the classic energies (variance of Laplacian, Tenengrad,
//!    modified Laplacian), across different photos;
//! 4. face focus on hand-placed faces: the eyes at a fixed scale
//!    (`eye_sharpness`) against the face region at native scale
//!    (`region_sharpness`), sharp, blurred, face-only blurred, half size;
//! 5. timings (release build): compute_metrics on a 3000×2000 preview,
//!    the face measures, group_similar and assess on 5 000 photos.

#[path = "../tests/common/mod.rs"]
mod common;

use cull_analysis::internals::{exposure_debug, focus_debug, FocusDebug};
use cull_analysis::{assess, compute_metrics, group_similar, PhotoFacts};
use cull_types::{CullSettings, ExifInfo, Sensitivity, TechnicalMetrics, Verdict};
use image::RgbImage;
use std::path::{Path, PathBuf};
use std::time::Instant;

struct Variant {
    name: &'static str,
    /// Expected class: Some(true) sharp, Some(false) blurred, None not a focus case.
    sharp: Option<bool>,
    img: RgbImage,
}

fn variants(img: &RgbImage, seed: u64) -> Vec<Variant> {
    use common::*;
    let (w, h) = img.dimensions();
    // Blur radii are expressed at the 1024 px working scale.
    let k = w.max(h) as f32 / 1024.0;
    let angle = (seed % 180) as f32;
    vec![
        Variant { name: "original", sharp: Some(true), img: img.clone() },
        Variant { name: "jpeg q55", sharp: Some(true), img: jpeg_roundtrip(img, 55) },
        Variant { name: "noise σ8", sharp: Some(true), img: add_noise(img, 8.0, seed) },
        Variant { name: "blur σ0.8", sharp: None, img: gaussian_blur(img, 0.8 * k) },
        Variant { name: "blur σ1.5", sharp: Some(false), img: gaussian_blur(img, 1.5 * k) },
        Variant { name: "blur σ2.5", sharp: Some(false), img: gaussian_blur(img, 2.5 * k) },
        Variant { name: "blur σ4", sharp: Some(false), img: gaussian_blur(img, 4.0 * k) },
        Variant {
            name: "blur σ2.5+noise",
            sharp: Some(false),
            img: add_noise(&gaussian_blur(img, 2.5 * k), 8.0, seed + 1),
        },
        Variant { name: "motion 6px", sharp: None, img: motion_blur(img, 6.0 * k, angle) },
        Variant { name: "motion 12px", sharp: Some(false), img: motion_blur(img, 12.0 * k, angle) },
        Variant { name: "ev -1", sharp: None, img: shift_ev(img, -1.0) },
        Variant { name: "ev -2", sharp: None, img: shift_ev(img, -2.0) },
        Variant { name: "ev -3", sharp: None, img: shift_ev(img, -3.0) },
        Variant { name: "ev +1", sharp: None, img: shift_ev(img, 1.0) },
        Variant { name: "ev +2", sharp: None, img: shift_ev(img, 2.0) },
        Variant { name: "ev +3", sharp: None, img: shift_ev(img, 3.0) },
        Variant {
            name: "crop 96%",
            sharp: None,
            img: resize(&crop(img, w * 2 / 100, h * 2 / 100, w * 96 / 100, h * 96 / 100), w, h),
        },
        Variant {
            name: "crop 92%",
            sharp: None,
            img: resize(&crop(img, w * 4 / 100, h * 4 / 100, w * 92 / 100, h * 92 / 100), w, h),
        },
    ]
}

struct Row {
    photo: String,
    variant: &'static str,
    sharp: Option<bool>,
    dbg: FocusDebug,
    metrics: TechnicalMetrics,
    ev_under: f32,
    ev_over: f32,
}

fn auc(pos: &[f32], neg: &[f32]) -> f32 {
    if pos.is_empty() || neg.is_empty() {
        return f32::NAN;
    }
    let mut wins = 0f64;
    for &p in pos {
        for &n in neg {
            wins += if p > n {
                1.0
            } else if p == n {
                0.5
            } else {
                0.0
            };
        }
    }
    (wins / (pos.len() * neg.len()) as f64) as f32
}

fn fmt_w(w: Option<f32>) -> String {
    w.map(|v| format!("{v:.2}")).unwrap_or_else(|| "-".into())
}

fn load_photos(dir: &Path) -> Vec<(String, RgbImage)> {
    let mut paths: Vec<PathBuf> =
        std::fs::read_dir(dir).map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
    paths.retain(|p| {
        matches!(
            p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
            Some("jpg" | "jpeg" | "png")
        )
    });
    paths.sort();
    paths
        .into_iter()
        .filter_map(|p| {
            let img = image::open(&p).ok()?.to_rgb8();
            Some((p.file_stem()?.to_string_lossy().into_owned(), img))
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "--verbose");
    let dir = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../samples/photos")));

    let mut sources: Vec<(String, RgbImage)> =
        (1..=4).map(|s| (format!("synthetic-{s}"), common::scene(1536, 1024, s))).collect();
    let photos = load_photos(&dir);
    println!("photos: {} real from {} + {} synthetic", photos.len(), dir.display(), sources.len());
    sources.extend(photos);

    let mut rows: Vec<Row> = Vec::new();
    for (k, (name, img)) in sources.iter().enumerate() {
        for v in variants(img, k as u64 * 7 + 3) {
            let e = exposure_debug(&v.img);
            rows.push(Row {
                photo: name.clone(),
                variant: v.name,
                sharp: v.sharp,
                dbg: focus_debug(&v.img),
                metrics: compute_metrics(&v.img),
                ev_under: e.ev_under,
                ev_over: e.ev_over,
            });
        }
    }

    // Verdicts: each variant judged as a single (no grouping) on Balanced,
    // and the blur/exposure verdicts on the three sensitivities.
    let exif = ExifInfo::default();
    // Hashes cleared: variants of one photo hash alike, and this table is
    // about defects, not duplicates.
    let unhashed: Vec<TechnicalMetrics> = rows
        .iter()
        .map(|r| {
            let mut m = r.metrics;
            m.hashes = Default::default();
            m
        })
        .collect();
    let facts: Vec<PhotoFacts<'_>> = rows
        .iter()
        .enumerate()
        .map(|(i, _)| PhotoFacts {
            id: Box::leak(format!("{i:05}").into_boxed_str()),
            exif: &exif,
            metrics: &unhashed[i],
            faces: &[],
            embedding: None,
            embedding_space: Default::default(),
            aesthetic: None,
            subject: None,
        })
        .collect();
    let mut verdicts = Vec::new();
    for s in [Sensitivity::Strict, Sensitivity::Balanced, Sensitivity::Lenient] {
        let settings = CullSettings { sensitivity: s, group_similar: false, ..Default::default() };
        verdicts.push(assess(&facts, &[], &settings));
    }

    println!(
        "\n{:<28} {:<16} {:>5} {:>6} {:>6} {:>5} {:>5} {:>5} {:>6} {:>5} {:>5}  {:<8} reasons (Balanced) | S/B/L",
        "photo", "variant", "focus", "width", "edges", "noise", "motn", "ratio", "expo", "ev-", "ev+", "verdict"
    );
    let short = |v: Verdict| match v {
        Verdict::Keep => "K",
        Verdict::Maybe => "M",
        Verdict::Reject => "R",
    };
    for (i, r) in rows.iter().enumerate() {
        if !verbose && !r.photo.starts_with("synthetic") && r.variant != "original" {
            continue;
        }
        let a = &verdicts[1][i];
        println!(
            "{:<28} {:<16} {:>5.2} {:>6} {:>6} {:>5.2} {:>5.2} {:>5.2} {:>6.2} {:>5.1} {:>5.1}  {:<8} {:?} | {}/{}/{}",
            r.photo.chars().take(28).collect::<String>(),
            r.variant,
            r.metrics.sharpness.score,
            fmt_w(r.dbg.peak_width),
            r.dbg.edges,
            r.dbg.noise_blur_grad,
            r.metrics.sharpness.motion_blur,
            r.dbg.motion_ratio,
            r.metrics.exposure.score,
            r.ev_under,
            r.ev_over,
            format!("{:?}", a.verdict),
            a.reasons,
            short(verdicts[0][i].verdict),
            short(verdicts[1][i].verdict),
            short(verdicts[2][i].verdict),
        );
    }

    // Per-variant summary over real photos.
    let real: Vec<usize> = (0..rows.len()).filter(|&i| !rows[i].photo.starts_with("synthetic")).collect();
    if !real.is_empty() {
        println!(
            "\nPer-variant summary over {} real photos (median focus / width / motion / exposure; % Reject on Strict, Balanced, Lenient)",
            real.len() / variants(&RgbImage::new(4, 4), 0).len()
        );
        let names: Vec<&str> = variants(&RgbImage::new(4, 4), 0).iter().map(|v| v.name).collect();
        for name in names {
            let idx: Vec<usize> = real.iter().copied().filter(|&i| rows[i].variant == name).collect();
            let med = |mut v: Vec<f32>| {
                v.sort_by(|a, b| a.total_cmp(b));
                if v.is_empty() {
                    f32::NAN
                } else {
                    v[v.len() / 2]
                }
            };
            let pct = |s: usize, which: Verdict| {
                100.0 * idx.iter().filter(|&&i| verdicts[s][i].verdict == which).count() as f32
                    / idx.len().max(1) as f32
            };
            let motion_pct = 100.0
                * idx.iter().filter(|&&i| rows[i].metrics.sharpness.motion_blur >= 0.65).count() as f32
                / idx.len().max(1) as f32;
            println!(
                "  {:<16} focus {:>4.2}  width {:>4.2}  motion {:>4.2} (≥0.65: {:>3.0}%)  expo {:>4.2}   reject {:>3.0}% {:>3.0}% {:>3.0}%   maybe(B) {:>3.0}%",
                name,
                med(idx.iter().map(|&i| rows[i].metrics.sharpness.score).collect()),
                med(idx.iter().filter_map(|&i| rows[i].dbg.peak_width).collect()),
                med(idx.iter().map(|&i| rows[i].metrics.sharpness.motion_blur).collect()),
                motion_pct,
                med(idx.iter().map(|&i| rows[i].metrics.exposure.score).collect()),
                pct(0, Verdict::Reject),
                pct(1, Verdict::Reject),
                pct(2, Verdict::Reject),
                pct(1, Verdict::Maybe),
            );
        }
        // pHash / dHash distance of each variant to its original.
        println!("\npHash / dHash Hamming distance to the original (median, max) over real photos:");
        let names: Vec<&str> = variants(&RgbImage::new(4, 4), 0).iter().map(|v| v.name).collect();
        for name in names.iter().skip(1) {
            let mut dp = Vec::new();
            let mut dd = Vec::new();
            for &i in real.iter().filter(|&&i| rows[i].variant == *name) {
                let o =
                    real.iter().find(|&&j| rows[j].photo == rows[i].photo && rows[j].variant == "original").unwrap();
                let (a, b) = (rows[*o].metrics.hashes, rows[i].metrics.hashes);
                dp.push(cull_analysis::internals::hamming(a.phash, b.phash));
                dd.push(cull_analysis::internals::hamming(a.dhash, b.dhash));
            }
            dp.sort_unstable();
            dd.sort_unstable();
            println!(
                "  {:<16} pHash {:>2} / {:>2}   dHash {:>2} / {:>2}",
                name,
                dp[dp.len() / 2],
                dp[dp.len() - 1],
                dd[dd.len() / 2],
                dd[dd.len() - 1]
            );
        }
        let originals: Vec<usize> = real.iter().copied().filter(|&i| rows[i].variant == "original").collect();
        let mut cross = Vec::new();
        for (k, &i) in originals.iter().enumerate() {
            for &j in &originals[k + 1..] {
                cross.push(cull_analysis::internals::hamming(
                    rows[i].metrics.hashes.phash,
                    rows[j].metrics.hashes.phash,
                ));
            }
        }
        cross.sort_unstable();
        println!(
            "  different photos: pHash min {} / p5 {} / median {}",
            cross[0],
            cross[cross.len() / 20],
            cross[cross.len() / 2]
        );
        // Originals flagged: list them (to look at by eye).
        for &i in &real {
            if rows[i].variant == "original" && verdicts[1][i].verdict != Verdict::Keep {
                println!(
                    "  original not kept on Balanced: {} {:?} {:?}",
                    rows[i].photo, verdicts[1][i].verdict, verdicts[1][i].reasons
                );
            }
        }
    }

    // Measure benchmark: AUC sharp vs blurred across photos.
    let pick = |f: &dyn Fn(&Row) -> f32, class: bool| -> Vec<f32> {
        real.iter().map(|&i| &rows[i]).filter(|r| r.sharp == Some(class)).map(f).collect()
    };
    if !real.is_empty() {
        println!("\nSharp vs blurred separation across photos (AUC, 1 = perfect, 0.5 = chance):");
        type Measure = (&'static str, fn(&Row) -> f32);
        let measures: [Measure; 6] = [
            ("edge-width score (ours)", |r| r.metrics.sharpness.score),
            ("edge width peak (inverted)", |r| -r.dbg.peak_width.unwrap_or(9.0)),
            ("VoL global (noise-corr.)", |r| r.dbg.vol_global),
            ("VoL p90 tiles", |r| r.dbg.vol_peak),
            ("Tenengrad p90 tiles", |r| r.dbg.tenengrad_peak),
            ("modified Laplacian p90", |r| r.dbg.modlap_peak),
        ];
        let by_variant = |f: &dyn Fn(&Row) -> f32, v: &str| -> Vec<f32> {
            real.iter().map(|&i| &rows[i]).filter(|r| r.variant == v).map(f).collect()
        };
        println!("  {:<28} {:>9} {:>14} {:>16}", "", "all", "orig vs σ1.5", "noisy vs σ2.5+n");
        for (name, f) in measures {
            println!(
                "  {:<28} {:>9.3} {:>14.3} {:>16.3}",
                name,
                auc(&pick(&f, true), &pick(&f, false)),
                auc(&by_variant(&f, "original"), &by_variant(&f, "blur σ1.5")),
                auc(&by_variant(&f, "noise σ8"), &by_variant(&f, "blur σ2.5+noise")),
            );
        }
    }

    faces(&sources);
    bench(&sources);
}

/// Faces placed by hand on some calibration photos (normalised x, y, w, h;
/// approximate; eyes assumed at 32% / 68% of the width, 38% of the height):
/// eye focus and region focus on the original, on whole-frame blur, with
/// only the face blurred (missed focus, background sharp), and on the
/// original at half size (the eye focus must not move, the region did).
const FACES: &[(&str, [f32; 4])] = &[
    ("portrait-bokeh-56mm-a", [0.25, 0.30, 0.55, 0.38]),
    ("portrait-bokeh-56mm-b", [0.27, 0.25, 0.50, 0.35]),
    ("event-candid-portrait", [0.56, 0.22, 0.10, 0.08]),
    ("wedding-reception-portrait", [0.58, 0.12, 0.16, 0.28]),
    ("wedding-family-group", [0.18, 0.19, 0.05, 0.09]),
];

/// A hand-placed face (see `FACES`).
fn hand_face(r: &[f32; 4]) -> cull_types::FaceObservation {
    use cull_types::{NormPoint, NormRect};
    let eye = |fx: f32| NormPoint { x: r[0] + fx * r[2], y: r[1] + 0.38 * r[3] };
    cull_types::FaceObservation {
        bbox: NormRect { x: r[0], y: r[1], w: r[2], h: r[3] },
        confidence: 1.0,
        landmarks: [eye(0.32), eye(0.68), NormPoint::default(), NormPoint::default(), NormPoint::default()],
        right_eye_open: None,
        left_eye_open: None,
        sharpness: None,
        area_ratio: r[2] * r[3],
        mesh_presence: None,
        pose: None,
        expression: None,
        skin_luma: None,
        skin_clipped: None,
        body_hash: None,
    }
}

fn faces(sources: &[(String, RgbImage)]) {
    use cull_analysis::{eye_sharpness, region_sharpness};
    use cull_types::NormRect;
    println!("\nFace focus on hand-placed faces: eyes at 64 px apart / face region at native size [frame score]:");
    println!(
        "  {:<28} {:>18} {:>18} {:>18} {:>18} {:>18}",
        "photo", "original", "blur σ1.5", "blur σ2.5", "face-only σ2.5", "original ½ size"
    );
    for (name, r) in FACES {
        let Some((_, img)) = sources.iter().find(|(n, _)| n == name) else { continue };
        let (w, h) = img.dimensions();
        let k = w.max(h) as f32 / 1024.0;
        let rect = NormRect { x: r[0], y: r[1], w: r[2], h: r[3] };
        let face = hand_face(r);
        // Face-only blur: blurred copy pasted inside the (slightly enlarged) box.
        let blurred = common::gaussian_blur(img, 2.5 * k);
        let mut missed = img.clone();
        let (x0, y0) = (((r[0] - 0.02) * w as f32).max(0.0) as u32, ((r[1] - 0.02) * h as f32).max(0.0) as u32);
        let (x1, y1) =
            ((((r[0] + r[2] + 0.02) * w as f32) as u32).min(w), (((r[1] + r[3] + 0.02) * h as f32) as u32).min(h));
        for y in y0..y1 {
            for x in x0..x1 {
                missed.put_pixel(x, y, *blurred.get_pixel(x, y));
            }
        }
        let cell = |im: &RgbImage| {
            let eyes = eye_sharpness(im, &face).map_or("-".into(), |v| format!("{v:.2}"));
            format!("{eyes} / {:.2} [{:.2}]", region_sharpness(im, rect), compute_metrics(im).sharpness.score)
        };
        println!(
            "  {:<28} {:>18} {:>18} {:>18} {:>18} {:>18}",
            name,
            cell(img),
            cell(&common::gaussian_blur(img, 1.5 * k)),
            cell(&blurred),
            cell(&missed),
            cell(&common::resize(img, w / 2, h / 2))
        );
    }
}

fn bench(sources: &[(String, RgbImage)]) {
    println!("\nTimings ({} build):", if cfg!(debug_assertions) { "DEBUG" } else { "release" });
    let base = &sources.last().expect("at least one source").1;
    let big = common::resize(base, 3000, 2000);
    let mid = common::resize(base, 1620, 1080);
    for (label, img) in [("3000×2000", &big), ("1620×1080", &mid)] {
        let _ = compute_metrics(img);
        let n = 30;
        let mut times: Vec<f64> = (0..n)
            .map(|_| {
                let t = Instant::now();
                std::hint::black_box(compute_metrics(std::hint::black_box(img)));
                t.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        times.sort_by(|a, b| a.total_cmp(b));
        println!("  compute_metrics {label}: min {:.2} ms, median {:.2} ms", times[0], times[n / 2]);
        let mut acc: Vec<(&str, f64)> = Vec::new();
        for _ in 0..n {
            for (k, (name, v)) in cull_analysis::internals::stage_timings(img).into_iter().enumerate() {
                if acc.len() <= k {
                    acc.push((name, 0.0));
                }
                acc[k].1 += v / n as f64;
            }
        }
        let parts: Vec<String> = acc.iter().map(|(k, v)| format!("{k} {v:.2}")).collect();
        println!("    stages: {}", parts.join(", "));
    }
    let face = cull_types::NormRect { x: 0.45, y: 0.3, w: 0.1, h: 0.15 };
    let t = Instant::now();
    for _ in 0..200 {
        std::hint::black_box(cull_analysis::region_sharpness(&big, face));
    }
    println!("  region_sharpness 300×300 face: {:.3} ms", t.elapsed().as_secs_f64() * 1000.0 / 200.0);
    let hand = hand_face(&[face.x, face.y, face.w, face.h]);
    let t = Instant::now();
    for _ in 0..200 {
        std::hint::black_box(cull_analysis::eye_sharpness(&big, &hand));
        std::hint::black_box(cull_analysis::skin_exposure(&big, &hand));
        std::hint::black_box(cull_analysis::body_hash(&big, &hand));
    }
    println!(
        "  eye_sharpness + skin_exposure + body_hash, 300×300 face: {:.3} ms",
        t.elapsed().as_secs_f64() * 1000.0 / 200.0
    );

    // 5 000-photo synthetic wedding for assess / group_similar.
    let n = 5000;
    let mut rng = common::Rng::new(42);
    let metrics: Vec<TechnicalMetrics> = (0..n)
        .map(|_| {
            let mut m = TechnicalMetrics::default();
            m.sharpness.score = rng.range(0.2, 1.0);
            m.exposure.score = rng.range(0.4, 1.0);
            m.exposure.mean_luma = rng.range(0.2, 0.7);
            m.hashes.phash = rng.next_u64();
            m.hashes.dhash = rng.next_u64();
            m
        })
        .collect();
    let mut t_ms = 0i64;
    let exifs: Vec<ExifInfo> = (0..n)
        .map(|_| {
            t_ms += (rng.range(0.2, 20.0) * 1000.0) as i64;
            ExifInfo { captured_at_ms: Some(t_ms), ..Default::default() }
        })
        .collect();
    let embeddings: Vec<Vec<f32>> = (0..n)
        .map(|_| {
            let v: Vec<f32> = (0..512).map(|_| rng.normal()).collect();
            let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            v.iter().map(|x| x / norm).collect()
        })
        .collect();
    let ids: Vec<String> = (0..n).map(|i| format!("{i:06}")).collect();
    let facts: Vec<PhotoFacts<'_>> = (0..n)
        .map(|i| PhotoFacts {
            id: &ids[i],
            exif: &exifs[i],
            metrics: &metrics[i],
            faces: &[],
            embedding: Some(&embeddings[i]),
            embedding_space: Default::default(),
            aesthetic: Some(rng.range(3.5, 7.0)),
            subject: None,
        })
        .collect();
    let settings = CullSettings::default();
    let t = Instant::now();
    let groups = group_similar(&facts, &settings);
    println!("  group_similar 5 000 photos (512-d embeddings): {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    let _ = assess(&facts, &groups, &settings);
    let t = Instant::now();
    let runs = 20;
    for _ in 0..runs {
        std::hint::black_box(assess(&facts, &groups, &settings));
    }
    println!("  assess 5 000 photos: {:.2} ms", t.elapsed().as_secs_f64() * 1000.0 / runs as f64);
}
