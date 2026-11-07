//! `pelli-cull` — command line of the Pelli Instant engine.
//!
//! ```text
//! pelli-cull analyze <folder> [--sensitivity strict|balanced|lenient] [--keep N] [--json] [--flat]
//!                    [--precision standard|maximum] [--threads N] [--background]
//! pelli-cull serve [--port 4317]       # HTTP dev bridge for the UI (VITE_BRIDGE_URL)
//! ```
//! Common options: `--data-dir <dir>` (library + cache, default
//! `./.pelli-cull`), `--models-dir <dir>` (default `./models`),
//! `--maximum-dir <dir>` (the Maximum pack; `analyze` defaults to
//! `<models-dir>/maximum`, `serve` to the data dir like the app).
//!
//! `analyze` is a developer tool: it needs no Pelli account and its photos
//! are never counted on a monthly quota. The dev bridge behaves like the app
//! (sign-in and quota, unless built with `PELLI_INSTANT_NO_ACCOUNT=1`).

mod bridge;

use cull_engine::cull_types::{Precision, Sensitivity};
use cull_engine::modelpack::ModelPackState;
use cull_engine::{Engine, EngineConfig, NullSink};
use std::path::PathBuf;
use std::sync::Arc;

struct Args {
    positional: Vec<String>,
    flags: Vec<(String, Option<String>)>,
}

impl Args {
    fn parse() -> Args {
        let mut positional = Vec::new();
        let mut flags = Vec::new();
        let mut it = std::env::args().skip(1).peekable();
        while let Some(a) = it.next() {
            if let Some(name) = a.strip_prefix("--") {
                let value = match it.peek() {
                    Some(v) if !v.starts_with("--") && takes_value(name) => it.next(),
                    _ => None,
                };
                flags.push((name.to_string(), value));
            } else {
                positional.push(a);
            }
        }
        Args { positional, flags }
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.flags.iter().find(|(n, _)| n == name).and_then(|(_, v)| v.as_deref())
    }

    fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(n, _)| n == name)
    }
}

fn takes_value(flag: &str) -> bool {
    matches!(
        flag,
        "sensitivity" | "data-dir" | "models-dir" | "port" | "keep" | "precision" | "threads" | "maximum-dir"
    )
}

fn usage() -> ! {
    eprintln!(
        "usage:\n  pelli-cull analyze <folder> [--sensitivity strict|balanced|lenient] [--keep N] [--json] [--flat]\n                     [--precision standard|maximum] [--threads N] [--background]\n      local batch analysis; no Pelli account, nothing counted on a quota\n  pelli-cull serve [--port 4317]\n      HTTP dev bridge for the UI (sign-in and quota like the app)\n\noptions: --data-dir <dir> --models-dir <dir> --maximum-dir <dir>"
    );
    std::process::exit(2)
}

fn engine(
    args: &Args,
    mapper: cull_engine::UrlMapper,
    events: Arc<dyn cull_engine::EventSink>,
    pack_dir: Option<PathBuf>,
) -> Arc<Engine> {
    let data_dir = PathBuf::from(args.value("data-dir").unwrap_or(".pelli-cull"));
    let models_dir = PathBuf::from(args.value("models-dir").unwrap_or("models"));
    match Engine::open(EngineConfig {
        cache_dir: data_dir.join("cache"),
        data_dir,
        pack_dir: args.value("maximum-dir").map(PathBuf::from).or(pack_dir),
        models_dir,
        url_mapper: mapper,
        events,
        secrets: None,
    }) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("cannot open the engine: {e}");
            std::process::exit(1)
        }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    match args.positional.first().map(String::as_str) {
        Some("analyze") => analyze(&args),
        Some("serve") => bridge::serve(&args),
        _ => usage(),
    }
}

fn analyze(args: &Args) {
    let Some(folder) = args.positional.get(1) else { usage() };
    let models_dir = PathBuf::from(args.value("models-dir").unwrap_or("models"));
    let engine = engine(
        args,
        Arc::new(|p: &std::path::Path| p.display().to_string()),
        Arc::new(NullSink),
        Some(models_dir.join("maximum")),
    );
    let mut patch = cull_engine::settings::SettingsPatch::default();
    if let Some(p) = args.value("precision") {
        patch.precision = Some(if p == "maximum" { Precision::Maximum } else { Precision::Standard });
    }
    if args.has("background") {
        patch.background = Some(true);
    }
    if let Err(e) = engine.set_settings(patch) {
        eprintln!("settings: {e}");
    }
    engine.set_worker_threads(args.value("threads").and_then(|t| t.parse().ok()));
    if engine.get_settings().precision == Precision::Maximum {
        // A pack folder never prepared with this data dir is checked first.
        let started = std::time::Instant::now();
        loop {
            let pack = engine.model_pack();
            match pack.state {
                ModelPackState::Preparing { .. } if started.elapsed().as_secs() < 600 => {
                    std::thread::sleep(std::time::Duration::from_millis(200))
                }
                ModelPackState::Ready => {
                    eprintln!("precision: maximum (pack checked in {:.1}s)", started.elapsed().as_secs_f32());
                    break;
                }
                other => {
                    eprintln!("note: Maximum pack not ready ({other:?}); analysing in standard precision");
                    break;
                }
            }
        }
    }

    let (caps, err) = engine.models_status();
    if let Some(e) = err {
        eprintln!("note: AI models unavailable ({e}); technical measures only");
    } else {
        eprintln!(
            "models: faces={} eyes={} embeddings={} aesthetic={} ({})",
            caps.faces,
            caps.eyes,
            caps.embeddings,
            caps.aesthetic,
            engine.models().map(|m| m.accelerator()).unwrap_or_default()
        );
    }
    let project = match engine.open_folder(std::path::Path::new(folder), !args.has("flat")) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1)
        }
    };
    let mut settings = project.settings.clone();
    if let Some(s) = args.value("sensitivity") {
        settings.sensitivity = match s {
            "strict" => Sensitivity::Strict,
            "lenient" => Sensitivity::Lenient,
            _ => Sensitivity::Balanced,
        };
    }
    if let Some(k) = args.value("keep").and_then(|k| k.parse().ok()) {
        settings.keep_per_group = k;
    }
    eprintln!("{} photo(s) in {}", project.photo_count, project.root_path);
    let summary = match engine.analyze_blocking(&project.id, settings) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("analysis failed: {e}");
            std::process::exit(1)
        }
    };
    let data = engine.get_project(&project.id).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    if args.has("json") {
        println!("{}", serde_json::to_string_pretty(&data).unwrap_or_default());
        return;
    }
    println!("{:<44} {:>5} {:<8} {:>5} {:>5} {:>4}  reasons", "photo", "score", "verdict", "focus", "expo", "grp");
    for p in &data.photos {
        let (score, verdict, focus, expo, group, reasons) = match &p.assessment {
            Some(a) => (
                format!("{:.0}", a.score),
                format!("{:?}", a.verdict).to_lowercase(),
                format!("{:.2}", a.breakdown.focus),
                format!("{:.2}", a.breakdown.exposure),
                a.group_id.map(|g| format!("{g}{}", if a.group_rank == 0 { "*" } else { "" })).unwrap_or_default(),
                a.reasons.iter().map(|r| cull_engine::view::reason_key(*r)).collect::<Vec<_>>().join(","),
            ),
            None => (
                "-".into(),
                p.error.clone().unwrap_or_else(|| "pending".into()),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ),
        };
        let name: String = p.rel_path.chars().rev().take(44).collect::<Vec<_>>().into_iter().rev().collect();
        println!("{name:<44} {score:>5} {verdict:<8} {focus:>5} {expo:>5} {group:>4}  {reasons}");
    }
    let secs = summary.duration_ms as f64 / 1000.0;
    println!(
        "\n{} analysed, {} failed in {:.1}s ({:.1} photos/s) — selected {}, to review {}, rejected {}, {} group(s), {:?} precision",
        summary.analyzed,
        summary.failed,
        secs,
        if secs > 0.0 { summary.analyzed as f64 / secs } else { 0.0 },
        summary.counts.selected,
        summary.counts.to_review,
        summary.counts.rejected,
        summary.groups,
        summary.precision
    );
}
