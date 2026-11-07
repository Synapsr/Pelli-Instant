//! Preview extraction + decode timings on real files.
//!
//! ```text
//! cargo run --release -p cull-decode --example bench_decode -- [FILES|DIRS] [--min 1440] [--iters 7]
//! # single core:
//! RAYON_NUM_THREADS=1 taskset -c 0 cargo run --release -p cull-decode --example bench_decode
//! ```
//!
//! Defaults to `samples/raw` (see `scripts/fetch-samples.sh`). For each file
//! it prints the embedded previews, the one `load_preview` picks, the bytes
//! read to find it, and the median time of `read_exif`, `load_preview` and
//! its parts. It then compares decoders on the chosen JPEG: jpeg-decoder with
//! DCT scaling (what the crate uses) against zune-jpeg full decode followed
//! by a SIMD resize to the same size, and times resize/encode helpers.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use cull_decode::{
    best_jpeg_bytes, decode_jpeg, encode_jpeg, load_preview, probe_previews, read_exif, resize_long_edge,
};
use cull_types::{FileKind, SourceFile};

fn median(mut v: Vec<Duration>) -> f64 {
    v.sort();
    v[v.len() / 2].as_secs_f64() * 1000.0
}

/// Median time in ms (after a warm-up run that fills the page cache).
fn time<T>(iters: usize, f: impl FnMut() -> T) -> (f64, T) {
    let (med, _, out) = time_min(iters, f);
    (med, out)
}

/// (median, minimum) in ms. The minimum is the most stable figure on a busy
/// machine.
fn time_min<T>(iters: usize, mut f: impl FnMut() -> T) -> (f64, f64, T) {
    let mut out = f();
    let mut ts = Vec::with_capacity(iters);
    for _ in 0..iters {
        let t = Instant::now();
        out = f();
        ts.push(t.elapsed());
    }
    let min = ts.iter().min().map_or(0.0, |d| d.as_secs_f64() * 1000.0);
    (median(ts), min, out)
}

fn collect(args: &[String]) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let inputs: Vec<PathBuf> = if args.is_empty() {
        vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../../samples/raw")]
    } else {
        args.iter().map(PathBuf::from).collect()
    };
    for p in inputs {
        if p.is_dir() {
            let mut v: Vec<PathBuf> = std::fs::read_dir(&p)
                .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect())
                .unwrap_or_default();
            v.sort();
            paths.extend(v);
        } else {
            paths.push(p);
        }
    }
    paths.retain(|p| p.extension().and_then(|e| e.to_str()).and_then(FileKind::from_extension).is_some());
    paths
}

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut opt = |name: &str, default: usize| -> usize {
        match args.iter().position(|a| a == name) {
            Some(i) if i + 1 < args.len() => {
                let v = args[i + 1].parse().unwrap_or(default);
                args.drain(i..=i + 1);
                v
            }
            _ => default,
        }
    };
    let min = opt("--min", 1440) as u32;
    let iters = opt("--iters", 7).max(1);
    let files = collect(&args);
    if files.is_empty() {
        println!("no input files (run scripts/fetch-samples.sh first)");
        return;
    }
    println!("min_long_edge = {min}, {iters} iterations, median times in ms\n");
    println!(
        "{:<34} {:>11} {:>9} {:>7} {:>7} {:>15} {:>11} {:>9}",
        "file", "chosen", "read KiB", "exif", "probe", "total med/min", "output", "best_jpeg"
    );
    let mut totals = Vec::new();
    let mut comparisons = Vec::new();
    for path in &files {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let kind =
            path.extension().and_then(|e| e.to_str()).and_then(FileKind::from_extension).unwrap_or(FileKind::Raw);
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let file = SourceFile { path: path.clone(), kind, size, modified_ms: 0 };

        let (t_exif, exif) = time(iters, || read_exif(path));
        let (t_probe, probe) = time(iters, || probe_previews(path, min));
        let (t_total, t_total_min, preview) = time_min(iters, || load_preview(&file, min));
        let (t_best, best) = time(iters.min(3), || best_jpeg_bytes(&file));
        let chosen = probe.as_ref().ok().and_then(|p| p.chosen.clone());
        match &preview {
            Ok(p) => {
                println!(
                    "{:<34} {:>11} {:>9} {:>7.2} {:>7.2} {:>15} {:>11} {:>9}",
                    name,
                    chosen.as_ref().map(|c| format!("{}x{}", c.width, c.height)).unwrap_or_else(|| "file".into()),
                    probe.as_ref().map(|p| format!("{}", p.bytes_read / 1024)).unwrap_or_else(|_| "-".into()),
                    t_exif,
                    t_probe,
                    format!("{t_total:.2}/{t_total_min:.2}"),
                    format!("{}x{}", p.image.width(), p.image.height()),
                    match &best {
                        Ok(b) => format!("{:.1}ms", t_best).replace("ms", "") + &format!("/{}K", b.len() / 1024),
                        Err(_) => "err".into(),
                    }
                );
                totals.push((name.clone(), t_total_min));
            }
            Err(e) => println!("{:<34} error: {e}", name),
        }
        if let Ok(probe) = &probe {
            let list: Vec<String> = probe
                .previews
                .iter()
                .map(|p| format!("{}x{}{} {}K", p.width, p.height, if p.is_jpeg { "" } else { " rgb" }, p.len / 1024))
                .collect();
            println!("    previews: {}", list.join(", "));
        }
        if let Ok(e) = &exif {
            println!(
                "    exif: {} {} | serial {} | lens {} | ISO {:?} | {:?}s f/{:?} {:?}mm | o{} | {}x{} | t={:?} | rating {:?} | gps {}",
                e.camera_make.as_deref().unwrap_or("?"),
                e.camera_model.as_deref().unwrap_or("?"),
                e.camera_serial.as_deref().unwrap_or("-"),
                e.lens.as_deref().unwrap_or("-"),
                e.iso,
                e.exposure_time_s,
                e.f_number,
                e.focal_length_mm,
                e.orientation,
                e.width.unwrap_or(0),
                e.height.unwrap_or(0),
                e.captured_at_ms,
                e.rating,
                e.gps.map(|g| format!("{:.4},{:.4}", g.latitude, g.longitude)).unwrap_or_else(|| "-".into()),
            );
        }
        // Decoder comparison on the chosen and the largest embedded JPEG.
        if let Ok(probe) = &probe {
            let largest = probe.previews.iter().filter(|p| p.is_jpeg).max_by_key(|p| p.width as u64 * p.height as u64);
            for (label, p) in [("chosen", probe.chosen.as_ref()), ("largest", largest)] {
                let Some(p) = p.filter(|p| p.is_jpeg) else { continue };
                if label == "largest" && Some(p) == probe.chosen.as_ref() {
                    continue;
                }
                if let Some(c) = compare_decoders(path, p.offset, p.len, min, iters) {
                    comparisons.push((format!("{name} [{label} {}x{}]", p.width, p.height), c));
                }
            }
        }
    }

    if !comparisons.is_empty() {
        println!("\nDecoder comparison (ms; ours/jd-scaled = min, others = median): cull-decode vs jpeg-decoder DCT-scaled/full vs zune-jpeg full (+ resize)");
        println!("{:<52} {:>8} {:>9} {:>8} {:>8} {:>8}", "jpeg", "ours", "jd-scaled", "jd-full", "zune", "zune+rsz");
        for (n, c) in &comparisons {
            println!(
                "{:<52} {:>8.2} {:>9.2} {:>8.2} {:>8.2} {:>8.2}",
                n, c.ours, c.jd_scaled, c.jd_full, c.zune, c.zune_resize
            );
        }
    }

    // Resize / encode helpers on a typical 1620x1080 preview.
    let img = image::RgbImage::from_fn(1620, 1080, |x, y| {
        image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x ^ y) % 256) as u8])
    });
    let (t_fir, _) = time(iters, || resize_long_edge(&img, 1024));
    let (t_tri, _) = time(iters, || image::imageops::resize(&img, 1024, 683, image::imageops::FilterType::Triangle));
    let (t_cr, _) = time(iters, || image::imageops::resize(&img, 1024, 683, image::imageops::FilterType::CatmullRom));
    let small = resize_long_edge(&img, 512);
    let (t_enc, _) = time(iters, || encode_jpeg(&small, 85));
    let (t_enc_img, _) = time(iters, || {
        let mut out = Vec::new();
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
        let _ = enc.encode(small.as_raw(), small.width(), small.height(), image::ExtendedColorType::Rgb8);
        out
    });
    println!("\nresize 1620x1080 -> 1024: fast_image_resize CatmullRom {t_fir:.2} ms | image Triangle {t_tri:.2} ms | image CatmullRom {t_cr:.2} ms");
    println!("encode 512x341 q85: jpeg-encoder {t_enc:.2} ms | image {t_enc_img:.2} ms");

    // Throughput the way the engine runs: every core busy, one file each.
    {
        use rayon::prelude::*;
        let sources: Vec<SourceFile> = files
            .iter()
            .map(|p| {
                let kind =
                    p.extension().and_then(|e| e.to_str()).and_then(FileKind::from_extension).unwrap_or(FileKind::Raw);
                SourceFile { path: p.clone(), kind, size: 0, modified_ms: 0 }
            })
            .collect();
        let rounds = 8;
        let t = Instant::now();
        let ok: usize = (0..rounds)
            .into_par_iter()
            .flat_map(|_| sources.par_iter())
            .map(|f| usize::from(load_preview(f, min).is_ok()))
            .sum();
        let s = t.elapsed().as_secs_f64();
        println!(
            "\nthroughput: {ok} previews in {s:.2} s on {} threads = {:.0} photos/s",
            rayon::current_num_threads(),
            ok as f64 / s
        );
    }

    if !totals.is_empty() {
        let mut v: Vec<f64> = totals.iter().map(|t| t.1).collect();
        v.sort_by(|a, b| a.total_cmp(b));
        println!(
            "\nload_preview (min of runs): median {:.2} ms, max {:.2} ms over {} files",
            v[v.len() / 2],
            v[v.len() - 1],
            v.len()
        );
    }
}

struct Comparison {
    ours: f64,
    jd_scaled: f64,
    jd_full: f64,
    zune: f64,
    zune_resize: f64,
}

fn compare_decoders(path: &Path, offset: u64, len: u64, min: u32, iters: usize) -> Option<Comparison> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut data = vec![0u8; len as usize];
    f.read_exact(&mut data).ok()?;

    let scaled = |full: bool| {
        let mut d = jpeg_decoder::Decoder::new(&data[..]);
        d.read_info().ok()?;
        let info = d.info()?;
        if !full {
            let long = info.width.max(info.height) as u32;
            let mut den = 1;
            for k in [8u32, 4, 2] {
                if long.div_ceil(k) >= min {
                    den = k;
                    break;
                }
            }
            if den > 1 {
                d.scale((info.width as u32).div_ceil(den) as u16, (info.height as u32).div_ceil(den) as u16).ok()?;
            }
        }
        let px = d.decode().ok()?;
        let info = d.info()?;
        Some((px, info.width as u32, info.height as u32))
    };
    let (_, ours, _) = time_min(iters, || decode_jpeg(&data, min));
    let (_, jd_scaled, out) = time_min(iters, || scaled(false));
    let (_, ow, oh) = out?;
    let (jd_full, _) = time(iters.min(3), || scaled(true));
    let zune = || {
        let mut d = zune_jpeg::JpegDecoder::new(std::io::Cursor::new(&data[..]));
        let px = d.decode().ok()?;
        let info = d.info()?;
        Some((px, info.width as u32, info.height as u32))
    };
    let (t_zune, _) = time(iters.min(3), zune);
    let (t_zune_resize, _) = time(iters.min(3), || {
        let (px, w, h) = zune()?;
        let img = image::RgbImage::from_raw(w, h, px)?;
        let (nw, nh) = if w >= h { (ow.max(oh), (h as u64 * ow.max(oh) as u64 / w as u64) as u32) } else { (ow, oh) };
        let src =
            fast_image_resize::images::ImageRef::new(w, h, img.as_raw(), fast_image_resize::PixelType::U8x3).ok()?;
        let mut dst = fast_image_resize::images::Image::new(nw.max(1), nh.max(1), fast_image_resize::PixelType::U8x3);
        let opts = fast_image_resize::ResizeOptions::new()
            .resize_alg(fast_image_resize::ResizeAlg::Convolution(fast_image_resize::FilterType::Bilinear));
        fast_image_resize::Resizer::new().resize(&src, &mut dst, &opts).ok()?;
        Some(dst.into_vec())
    });
    Some(Comparison { ours, jd_scaled, jd_full, zune: t_zune, zune_resize: t_zune_resize })
}
