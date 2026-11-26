//! The Maximum pack's sanity check (`MaximumModels::check`), by hand, and
//! the tool that computes its reference outputs (`assets/maximum-check.json`).
//!
//! ```text
//! cargo run --release -p cull-ml --example maximum_check -- DIR
//! cargo run --release -p cull-ml --example maximum_check -- --file MODEL FILE.onnx
//! cargo run --release -p cull-ml --example maximum_check -- --references DIR
//! ```
//!
//! * `DIR`: loads the pack from `DIR` (checksums verified), prints each
//!   model's correlation with its reference and tolerance, and runs
//!   `MaximumModels::check`. Exits 1 when it fails.
//! * `--file MODEL FILE`: runs one ONNX file, not verified, in the place of
//!   a model of the pack (`dinov2`, `clip-l14-vision`, `clip-l14-text`,
//!   `rfdetr`, `retinaface`, or a manifest id): would this variant pass on
//!   this processor? Exits 1 when it would not (the int8 DINOv2 on x64).
//! * `--references DIR`: computes the references from the full-precision
//!   exports in `DIR` and writes them into `assets/maximum-check.json`.
//!   fp32 numerics barely depend on the processor: any machine will do,
//!   CPU only. The files, by their names in `DIR`:
//!   - `dinov2-base-fp32.onnx`: <https://huggingface.co/onnx-community/dinov2-base-ONNX/resolve/9a1732c724ef0847dcedf00d3d5a93f61dea0370/onnx/model.onnx>
//!   - `clip-vit-l14-vision-fp32.onnx`: <https://huggingface.co/Xenova/clip-vit-large-patch14/resolve/c307790166907339eed5a9a53a249af534102536/onnx/vision_model.onnx>
//!   - `clip-vit-l14-text-fp32.onnx`: the same folder's `text_model.onnx`
//!   - `rfdetr-seg-small-384-w16.onnx`, `retinaface-r50-w16.onnx`: the pack's
//!     own files (fp32 computation of fp16-stored weights; Pelli's
//!     conversions, no other export).

use cull_ml::{Acceleration, MaximumModels, RuntimeOptions};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const JSON: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/maximum-check.json");

/// Family → full-precision file the references come from.
const FULL_PRECISION: [(&str, &str); 5] = [
    ("dinov2", "dinov2-base-fp32.onnx"),
    ("clip-l14-vision", "clip-vit-l14-vision-fp32.onnx"),
    ("clip-l14-text", "clip-vit-l14-text-fp32.onnx"),
    ("rfdetr", "rfdetr-seg-small-384-w16.onnx"),
    ("retinaface", "retinaface-r50-w16.onnx"),
];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--file") => match (args.get(1), args.get(2)) {
            (Some(model), Some(file)) => try_file(model, Path::new(file)),
            _ => usage(),
        },
        Some("--references") => match args.get(1) {
            Some(dir) => references(Path::new(dir)),
            None => usage(),
        },
        Some(dir) if !dir.starts_with("--") => check_pack(&PathBuf::from(dir)),
        _ => usage(),
    }
}

fn try_file(model: &str, file: &Path) -> ExitCode {
    match cull_ml::check_model_file(model, file) {
        Ok((c, min)) => {
            let pass = c >= min;
            let verdict = if pass { "pass" } else { "REJECTED" };
            println!(
                "{}: {model} from {}: correlation {c:.5}, at least {min}: {verdict}",
                cull_ml::arch(),
                file.display()
            );
            if pass {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn check_pack(dir: &Path) -> ExitCode {
    let opts = RuntimeOptions { acceleration: Acceleration::CpuOnly, ..Default::default() };
    let models = match MaximumModels::load(dir, &opts) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
    };
    let (outputs, refs) = match (models.check_outputs(), cull_ml::check_references()) {
        (Ok(o), Ok(r)) => (o, r),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    for (id, got) in &outputs {
        let reference = refs.iter().find(|(family, _)| id.starts_with(family.as_str())).map(|(_, v)| v);
        match reference {
            Some(r) => println!(
                "{}: {id:<22} correlation {:.5} (at least {})",
                cull_ml::arch(),
                correlation(got, r),
                cull_ml::min_correlation(id)
            ),
            None => println!("{id}: no reference"),
        }
    }
    let check = models.check(&|_| {});
    println!("MaximumModels::check: {}", check.as_ref().map(|_| "pass".to_string()).unwrap_or_else(|e| e.to_string()));
    if check.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn references(dir: &Path) -> ExitCode {
    let mut refs = serde_json::Map::new();
    for (family, file) in FULL_PRECISION {
        let path = dir.join(file);
        match cull_ml::check_file_outputs(family, &path) {
            Ok((_, v)) => {
                let v: Vec<f64> = v.iter().map(|x| (*x as f64 * 1e6).round() / 1e6).collect();
                println!("{family}: {} values from {file}", v.len());
                refs.insert(family.to_string(), serde_json::json!(v));
            }
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }
    let json = serde_json::json!({
        "comment": "Reference outputs of MaximumModels::check (crates/cull-ml/src/maximum.rs): each model family's full-precision (fp32) export run on the synthetic inputs, ONNX Runtime 1.31 on the CPU; valid on every processor, each model compared within its own tolerance (min_correlation). Recompute with: cargo run --release -p cull-ml --example maximum_check -- --references <folder of the fp32 exports>",
        "references": refs,
    });
    match std::fs::write(JSON, serde_json::to_string(&json).expect("json") + "\n") {
        Ok(()) => {
            println!("wrote {JSON}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{JSON}: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage() -> ExitCode {
    eprintln!("usage: maximum_check DIR | maximum_check --file MODEL FILE.onnx | maximum_check --references DIR");
    ExitCode::from(2)
}

/// Pearson correlation, as the check computes it.
fn correlation(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let n = a.len() as f64;
    let (ma, mb) = (a.iter().map(|x| *x as f64).sum::<f64>() / n, b.iter().map(|x| *x as f64).sum::<f64>() / n);
    let (mut ab, mut aa, mut bb) = (0f64, 0f64, 0f64);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = (*x as f64 - ma, *y as f64 - mb);
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    ab / (aa * bb).sqrt()
}
