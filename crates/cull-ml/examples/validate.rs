//! Validation and benchmark tool for cull-ml.
//!
//! ```text
//! cargo run --release -p cull-ml --example validate -- faces   IMG...
//! cargo run --release -p cull-ml --example validate -- similar IMG...
//! cargo run --release -p cull-ml --example validate -- search  "query" IMG...
//! cargo run --release -p cull-ml --example validate -- bench   THREADS ROUNDS IMG...
//! cargo run --release -p cull-ml --example validate -- json    IMG...
//! ```
//! Models are read from `PELLI_CULL_MODELS` or `models`.

use cull_ml::{Acceleration, ModelSet, RuntimeOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

fn models_dir() -> PathBuf {
    std::env::var_os("PELLI_CULL_MODELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../models"))
}

fn load(path: &str) -> image::RgbImage {
    image::open(path).unwrap_or_else(|e| panic!("{path}: {e}")).to_rgb8()
}

fn name(p: &str) -> String {
    Path::new(p).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn main() {
    env_logger_lite();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("faces");
    let opts = RuntimeOptions {
        acceleration: if std::env::var_os("CPU_ONLY").is_some() { Acceleration::CpuOnly } else { Acceleration::Auto },
        intra_threads: std::env::var("INTRA").ok().and_then(|s| s.parse().ok()).unwrap_or(1),
        text_search: mode == "search",
        checksum_cache_dir: None,
        accelerator_cache_dir: None,
        memory_bytes: None,
    };
    let t = Instant::now();
    let models = ModelSet::load(&models_dir(), &opts).expect("load models");
    eprintln!(
        "loaded {} in {:?}; capabilities {:?}; accelerator {}",
        models.models_id(),
        t.elapsed(),
        models.capabilities(),
        models.accelerator()
    );
    match mode {
        "faces" | "json" => {
            for p in &args[1..] {
                let img = load(p);
                let t = Instant::now();
                let obs = models.analyze(&img).expect("analyze");
                let dt = t.elapsed();
                if mode == "json" {
                    println!("{}\t{}", name(p), serde_json::to_string(&obs).unwrap());
                    continue;
                }
                println!(
                    "{} {}x{} faces={} aesthetic={:.2} {:?}",
                    name(p),
                    img.width(),
                    img.height(),
                    obs.faces.len(),
                    obs.aesthetic.unwrap_or(f32::NAN),
                    dt
                );
                for f in &obs.faces {
                    let iod = ((f.landmarks[1].x - f.landmarks[0].x) * img.width() as f32)
                        .hypot((f.landmarks[1].y - f.landmarks[0].y) * img.height() as f32);
                    let fmt = |v: Option<f32>| v.map(|v| format!("{v:.2}")).unwrap_or_else(|| "-".into());
                    println!(
                        "   conf={:.2} box=({:.3},{:.3},{:.3}x{:.3}) iod={:.0}px eyes R={} L={} closed@0.5={:?}",
                        f.confidence,
                        f.bbox.x,
                        f.bbox.y,
                        f.bbox.w,
                        f.bbox.h,
                        iod,
                        fmt(f.right_eye_open),
                        fmt(f.left_eye_open),
                        f.eyes_closed(0.5)
                    );
                }
            }
        }
        "emb" => {
            for p in &args[1..] {
                let o = models.analyze(&load(p)).expect("analyze");
                println!(
                    "{}\t{}\t{}",
                    name(p),
                    o.aesthetic.unwrap_or(f32::NAN),
                    serde_json::to_string(&o.embedding).unwrap()
                );
            }
        }
        "similar" => {
            let embs: Vec<(String, Vec<f32>)> = args[1..]
                .iter()
                .map(|p| (name(p), models.analyze(&load(p)).expect("analyze").embedding.expect("embedding")))
                .collect();
            for (i, (a, ea)) in embs.iter().enumerate() {
                for (b, eb) in embs.iter().skip(i + 1) {
                    println!("{:.4}\t{a}\t{b}", dot(ea, eb));
                }
            }
        }
        "search" => {
            let query = &args[1];
            let q = models.embed_text(query).expect("embed_text");
            let mut scored: Vec<(f32, String)> = args[2..]
                .iter()
                .map(|p| (dot(&q, &models.analyze(&load(p)).expect("analyze").embedding.expect("embedding")), name(p)))
                .collect();
            scored.sort_by(|a, b| b.0.total_cmp(&a.0));
            println!("query {query:?}");
            for (s, n) in scored.iter().take(8) {
                println!("  {s:.4}  {n}");
            }
        }
        "bench" => {
            let threads: usize = args[1].parse().expect("threads");
            let rounds: usize = args[2].parse().expect("rounds");
            let imgs: Vec<image::RgbImage> = args[3..].iter().map(|p| load(p)).collect();
            // Warm-up: one pass per thread so every pooled session exists.
            std::thread::scope(|s| {
                for _ in 0..threads {
                    s.spawn(|| models.analyze(&imgs[0]).expect("warm-up"));
                }
            });
            let total = imgs.len() * rounds;
            let next = AtomicUsize::new(0);
            let cpu_ns = AtomicUsize::new(0);
            let t = Instant::now();
            std::thread::scope(|s| {
                for _ in 0..threads {
                    s.spawn(|| {
                        let start = thread_cpu_ns();
                        loop {
                            let i = next.fetch_add(1, Ordering::Relaxed);
                            if i >= total {
                                break;
                            }
                            models.analyze(&imgs[i % imgs.len()]).expect("analyze");
                        }
                        cpu_ns.fetch_add(thread_cpu_ns().saturating_sub(start), Ordering::Relaxed);
                    });
                }
            });
            let dt = t.elapsed().as_secs_f64();
            println!(
                "threads={threads} photos={total} wall {:.1} ms/photo/thread, {:.2} photos/s; CPU {:.1} ms/photo",
                dt * 1000.0 * threads as f64 / total as f64,
                total as f64 / dt,
                cpu_ns.load(Ordering::Relaxed) as f64 / 1e6 / total as f64
            );
        }
        "scale" => {
            // Downscale each photo so that its largest face has the given
            // inter-ocular distance, then report the eye states of that face.
            let targets: Vec<f32> = args[1].split(',').map(|v| v.parse().expect("iod")).collect();
            for p in &args[2..] {
                let img = load(p);
                let obs = models.analyze(&img).expect("analyze");
                let Some(f) = obs.faces.first() else { continue };
                let iod = ((f.landmarks[1].x - f.landmarks[0].x) * img.width() as f32)
                    .hypot((f.landmarks[1].y - f.landmarks[0].y) * img.height() as f32);
                let mut line = format!("{}\tiod={iod:.0}", name(p));
                for &t in &targets {
                    let s = (t / iod).min(1.0);
                    let small = image::imageops::resize(
                        &img,
                        ((img.width() as f32 * s).round() as u32).max(1),
                        ((img.height() as f32 * s).round() as u32).max(1),
                        image::imageops::FilterType::Triangle,
                    );
                    let o = models.analyze(&small).expect("analyze");
                    // The same face: the largest one.
                    let fmt = |v: Option<f32>| v.map(|v| format!("{v:.2}")).unwrap_or_else(|| "-".into());
                    match o.faces.first() {
                        Some(g) => line.push_str(&format!("\t{t}:{}/{}", fmt(g.right_eye_open), fmt(g.left_eye_open))),
                        None => line.push_str(&format!("\t{t}:noface")),
                    }
                }
                println!("{line}");
            }
        }
        "resize" => {
            use fast_image_resize as fr;
            let img = load(&args[1]);
            let (w, h) = img.dimensions();
            for alg in [
                fr::ResizeAlg::Convolution(fr::FilterType::Box),
                fr::ResizeAlg::Convolution(fr::FilterType::Bilinear),
                fr::ResizeAlg::SuperSampling(fr::FilterType::Box, 2),
                fr::ResizeAlg::Interpolation(fr::FilterType::Bilinear),
            ] {
                let src = fr::images::ImageRef::new(w, h, img.as_raw(), fr::PixelType::U8x3).unwrap();
                let mut dst = fr::images::Image::new(1280, 853, fr::PixelType::U8x3);
                let mut r = fr::Resizer::new();
                let opts = fr::ResizeOptions::new().resize_alg(alg);
                let c0 = thread_cpu_ns();
                for _ in 0..5 {
                    r.resize(&src, &mut dst, &opts).unwrap();
                }
                println!("{alg:?}: {:.1} ms CPU", (thread_cpu_ns() - c0) as f64 / 5e6);
            }
        }
        other => panic!("unknown mode {other}"),
    }
}

/// CPU time of the calling thread (Linux; 0 elsewhere). With
/// `intra_threads = 1` all inference runs on the calling thread, so this is
/// the cost of `analyze` independent of other load on the machine.
fn thread_cpu_ns() -> usize {
    std::fs::read_to_string("/proc/thread-self/schedstat")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse().ok()))
        .unwrap_or(0)
}

/// Print `log` warnings to stderr without pulling a logger crate.
fn env_logger_lite() {
    struct L;
    impl log::Log for L {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= log::Level::Info
        }
        fn log(&self, r: &log::Record) {
            if self.enabled(r.metadata()) && r.target().starts_with("cull_ml") {
                eprintln!("[{}] {}", r.level(), r.args());
            }
        }
        fn flush(&self) {}
    }
    static LOGGER: L = L;
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
}
