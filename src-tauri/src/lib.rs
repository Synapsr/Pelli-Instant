//! Pelli Instant desktop shell.
//!
//! Thin layer over `cull-engine`: every command of `src/lib/contract.ts` is
//! routed to [`cull_engine::commands::dispatch`] by a single invoke handler
//! (but the app updates, answered by [`updates`]), engine events are
//! re-emitted as Tauri events, previews are served by the asset protocol
//! (scope = cache dir + opened project folders), the Pelli token lives in
//! the OS keychain, and the engine's services (telemetry, the daily licence
//! check) start with the app.
//!
//! Logs go to the terminal and to `pelli-instant.log` in the app's log
//! folder (macOS `~/Library/Logs/io.pelli.instant`, Windows
//! `%LOCALAPPDATA%\io.pelli.instant\logs`), 5 MB a file, the 3 latest kept:
//! a release build on Windows has no console to read them from.

mod updates;

use cull_engine::commands::{dispatch, error_json, Host};
use cull_engine::{Engine, EngineConfig, EngineError, EventSink, Result as EngineResult};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// Commands answered by the engine (names of `COMMANDS` in the contract,
/// but the update ones).
const COMMANDS: &[&str] = &[
    "app_info",
    "get_settings",
    "set_settings",
    "clear_cache",
    "auth_status",
    "auth_start",
    "auth_poll",
    "auth_logout",
    "refresh_account",
    "plus_status",
    "plus_offer",
    "plus_checkout",
    "plus_poll",
    "plus_cancel_checkout",
    "plus_activate",
    "plus_release",
    "telemetry_preview",
    "model_pack",
    "download_model_pack",
    "cancel_model_pack",
    "remove_model_pack",
    "pick_folder",
    "list_projects",
    "open_folder",
    "remove_project",
    "get_project",
    "start_analysis",
    "cancel_analysis",
    "recompute",
    "set_marks",
    "large_preview",
    "search",
    "stats_overview",
    "export_project",
    "list_galleries",
    "upload_check",
    "upload_to_pelli",
    "cancel_upload",
    "reveal_in_folder",
    "open_external",
];

/// `encodeURIComponent` keeps these characters (Tauri's `convertFileSrc`).
const URI_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

/// Same URL as `convertFileSrc(path)` in the webview.
fn asset_url(path: &Path) -> String {
    let encoded = utf8_percent_encode(&path.to_string_lossy(), URI_COMPONENT).to_string();
    if cfg!(target_os = "windows") {
        format!("http://asset.localhost/{encoded}")
    } else {
        format!("asset://localhost/{encoded}")
    }
}

struct TauriEvents(AppHandle);

impl EventSink for TauriEvents {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        if let Err(e) = self.0.emit(event, payload) {
            log::warn!("event {event} not delivered: {e}");
        }
    }
}

struct TauriHost(AppHandle);

fn host_err(e: impl std::fmt::Display) -> EngineError {
    EngineError::Remote { code: "internal".into(), message: e.to_string() }
}

impl Host for TauriHost {
    fn pick_folder(&self) -> Option<PathBuf> {
        self.0.dialog().file().blocking_pick_folder().and_then(|p| p.into_path().ok())
    }

    fn reveal(&self, path: &Path) -> EngineResult<()> {
        self.0.opener().reveal_item_in_dir(path).map_err(host_err)
    }

    fn open_external(&self, url: &str) -> EngineResult<()> {
        self.0.opener().open_url(url, None::<&str>).map_err(host_err)
    }

    fn device_name(&self) -> String {
        whoami::devicename().unwrap_or_else(|_| "Pelli Instant".into())
    }

    fn app_version(&self) -> String {
        self.0.package_info().version.to_string()
    }

    fn allow_directory(&self, dir: &Path) {
        if let Err(e) = self.0.asset_protocol_scope().allow_directory(dir, true) {
            log::warn!("asset scope: {e}");
        }
    }
}

/// Pelli token in the macOS Keychain / Windows Credential Manager.
#[cfg(any(target_os = "macos", target_os = "windows"))]
struct KeychainSecrets;

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl KeychainSecrets {
    fn entry() -> EngineResult<keyring::Entry> {
        keyring::Entry::new("io.pelli.instant", "pelli-account").map_err(host_err)
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl cull_engine::account::SecretStore for KeychainSecrets {
    fn get(&self) -> Option<String> {
        Self::entry().ok()?.get_password().ok()
    }
    fn set(&self, token: &str) -> EngineResult<()> {
        Self::entry()?.set_password(token).map_err(host_err)
    }
    fn delete(&self) -> EngineResult<()> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(host_err(e)),
        }
    }
}

/// The keychain in release builds. A debug build is signed anew at every
/// build, so macOS would ask for the keychain password at each launch: it
/// keeps the token in the local library instead, like Linux.
fn secrets() -> Option<Arc<dyn cull_engine::account::SecretStore>> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        if cfg!(debug_assertions) {
            None
        } else {
            Some(Arc::new(KeychainSecrets))
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// Models: `PELLI_CULL_MODELS`, else bundled resources, else `../models`
/// next to the sources (dev).
fn models_dir(app: &AppHandle) -> PathBuf {
    if let Ok(dir) = std::env::var("PELLI_CULL_MODELS") {
        return PathBuf::from(dir);
    }
    if let Ok(res) = app.path().resource_dir() {
        let bundled = res.join("models");
        if bundled.join("manifest.json").exists() {
            return bundled;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../models")
}

struct AppState {
    engine: Arc<Engine>,
    host: Arc<TauriHost>,
}

/// The log file and the terminal; `RUST_LOG=debug` (a single level) for
/// more.
fn log_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_log::{RotationStrategy, Target, TargetKind};
    let level = std::env::var("RUST_LOG").ok().and_then(|l| l.parse().ok()).unwrap_or(log::LevelFilter::Info);
    tauri_plugin_log::Builder::new()
        .clear_targets()
        .targets([
            Target::new(TargetKind::Stdout),
            Target::new(TargetKind::LogDir { file_name: Some("pelli-instant".into()) }),
        ])
        .level(level)
        .max_file_size(LOG_FILE_BYTES)
        .rotation_strategy(RotationStrategy::KeepSome(3))
        .build()
}

const LOG_FILE_BYTES: u128 = 5 * 1024 * 1024;

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    log::info!(
        "Pelli Instant {} on {} {}",
        env!("CARGO_PKG_VERSION"),
        cull_engine::pelli::os(),
        cull_engine::telemetry::arch()
    );
    let data_dir = handle.path().app_data_dir()?;
    let cache_dir = handle.path().app_cache_dir()?.join("previews");
    std::fs::create_dir_all(&cache_dir)?;
    handle.asset_protocol_scope().allow_directory(&cache_dir, true)?;
    let engine = Engine::open(EngineConfig {
        data_dir,
        cache_dir,
        models_dir: models_dir(&handle),
        pack_dir: None,
        url_mapper: Arc::new(|p: &Path| asset_url(p)),
        events: Arc::new(TauriEvents(handle.clone())),
        secrets: secrets(),
    })?;
    engine.start_services();
    app.manage(AppState { engine, host: Arc::new(TauriHost(handle.clone())) });
    updates::start(&handle);
    Ok(())
}

/// The commands of [`updates`].
fn update_command(invoke: tauri::ipc::Invoke) -> bool {
    let handler: fn(tauri::ipc::Invoke) -> bool =
        tauri::generate_handler![updates::update_status, updates::check_for_update, updates::install_update];
    handler(invoke)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(log_plugin())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(setup)
        .invoke_handler(|invoke| {
            let command = invoke.message.command().to_string();
            if !COMMANDS.contains(&command.as_str()) {
                return update_command(invoke);
            }
            let app = invoke.message.webview().app_handle().clone();
            let args = match invoke.message.payload() {
                tauri::ipc::InvokeBody::Json(v) => v.clone(),
                tauri::ipc::InvokeBody::Raw(_) => serde_json::Value::Null,
            };
            let resolver = invoke.resolver;
            tauri::async_runtime::spawn_blocking(move || {
                let state = app.state::<AppState>();
                match dispatch(&state.engine, state.host.as_ref(), &command, &args) {
                    Ok(v) => resolver.resolve(v),
                    Err(e) => {
                        log::warn!("{command}: {e}");
                        resolver.reject(error_json(&e))
                    }
                }
            });
            true
        })
        .run(tauri::generate_context!())
        .expect("error while running Pelli Instant");
}
