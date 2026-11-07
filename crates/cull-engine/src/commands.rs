//! Command dispatcher: one entry point for every command of
//! `src/lib/contract.ts`, shared by the Tauri shell and the HTTP dev bridge.
//! Arguments are the camelCase JSON object the UI sends.

use crate::settings::SettingsPatch;
use crate::view::{ExportOptions, MarkPatch};
use crate::{Engine, EngineError, Result};
use cull_types::CullSettings;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What only the host can do (native dialogs, OS integration).
pub trait Host: Send + Sync {
    fn pick_folder(&self) -> Option<PathBuf>;
    fn reveal(&self, path: &Path) -> Result<()>;
    fn open_external(&self, url: &str) -> Result<()>;
    /// Human name of this computer ("MacBook Pro de Léa").
    fn device_name(&self) -> String;
    /// `macos`, `windows`, `linux`, `web`.
    fn platform(&self) -> &'static str {
        crate::pelli::platform()
    }
    fn app_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
    /// Called with every folder whose files the UI may display (project
    /// roots), so the host can extend its file-serving scope.
    fn allow_directory(&self, _dir: &Path) {}
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    faces: bool,
    eyes: bool,
    embeddings: bool,
    aesthetic: bool,
    text_search: bool,
    /// The Maximum pack is installed and its models load.
    maximum: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    platform: &'static str,
    arch: &'static str,
    accelerator: String,
    models_ready: bool,
    capabilities: Capabilities,
    cpu_threads: usize,
    /// Total memory of the computer (bytes), `null` when unknown.
    memory_bytes: Option<u64>,
    cpu_cores: usize,
    /// Built with `PELLI_INSTANT_NO_ACCOUNT=1` (see the crate docs).
    no_account: bool,
}

fn arg<T: DeserializeOwned>(args: &Value, name: &str) -> Result<T> {
    let v = args.get(name).cloned().unwrap_or(Value::Null);
    serde_json::from_value(v)
        .map_err(|e| EngineError::Remote { code: "invalidArgument".into(), message: format!("{name}: {e}") })
}

fn ok<T: Serialize>(v: T) -> Result<Value> {
    Ok(serde_json::to_value(v)?)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadRequest {
    pub gallery_id: Option<String>,
    pub new_gallery: Option<NewGallery>,
    pub which: UploadWhich,
    pub folder_per_scene: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewGallery {
    pub title: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UploadWhich {
    Selected,
    SelectedAndToReview,
}

/// Run one command. Blocking: hosts call it from a worker thread.
/// Commands that talk to Pelli. A build without account never sends them, so
/// it never contacts Pelli at all (the UI does not offer them either).
const PELLI_COMMANDS: &[&str] =
    &["auth_start", "auth_poll", "refresh_account", "get_offers", "list_galleries", "upload_to_pelli"];

pub fn dispatch(engine: &Arc<Engine>, host: &dyn Host, command: &str, args: &Value) -> Result<Value> {
    if crate::no_account_build() && PELLI_COMMANDS.contains(&command) {
        return Err(EngineError::Remote {
            code: "noAccount".into(),
            message: "this build runs without a Pelli account".into(),
        });
    }
    match command {
        "app_info" => {
            let (caps, _err) = engine.models_status();
            ok(AppInfo {
                version: host.app_version(),
                platform: host.platform(),
                arch: std::env::consts::ARCH,
                accelerator: engine.models().map(|m| m.accelerator()).unwrap_or_else(|| "CPU".into()),
                models_ready: caps.faces,
                capabilities: Capabilities {
                    faces: caps.faces,
                    eyes: caps.eyes,
                    embeddings: caps.embeddings,
                    aesthetic: caps.aesthetic,
                    text_search: caps.text_search,
                    maximum: engine.maximum_ready(),
                },
                cpu_threads: engine.worker_threads(),
                memory_bytes: crate::system::total_memory_bytes(),
                cpu_cores: crate::system::cpu_cores(),
                no_account: crate::no_account_build(),
            })
        }
        "get_settings" => ok(engine.get_settings()),
        "set_settings" => {
            let patch: SettingsPatch = arg(args, "patch")?;
            ok(engine.set_settings(patch)?)
        }
        "clear_cache" => {
            engine.clear_cache()?;
            ok(engine.get_settings())
        }

        "model_pack" => ok(engine.model_pack()),
        "download_model_pack" => {
            engine.download_model_pack()?;
            Ok(Value::Null)
        }
        "cancel_model_pack" => ok(engine.cancel_model_pack()),
        "remove_model_pack" => ok(engine.remove_model_pack()?),

        "auth_status" => ok(engine.auth_status()),
        "auth_start" => {
            let state = engine.auth_start(&host.device_name())?;
            if let crate::account::AuthState::Pending { auth_url, .. } = &state {
                host.open_external(auth_url)?;
            }
            ok(state)
        }
        "auth_poll" => ok(engine.auth_poll()?),
        "auth_logout" => ok(engine.auth_logout()?),
        "refresh_account" => ok(engine.refresh_account()?),
        "get_offers" => ok(engine.get_offers(&arg::<String>(args, "lang")?)?),

        "pick_folder" => ok(host.pick_folder().map(|p| p.to_string_lossy().into_owned())),
        "list_projects" => {
            let projects = engine.list_projects()?;
            for p in &projects {
                host.allow_directory(Path::new(&p.root_path));
            }
            ok(projects)
        }
        "open_folder" => {
            let path: String = arg(args, "path")?;
            let recursive: bool = arg::<Option<bool>>(args, "recursive")?.unwrap_or(true);
            let summary = engine.open_folder(Path::new(&path), recursive)?;
            host.allow_directory(Path::new(&summary.root_path));
            ok(summary)
        }
        "remove_project" => {
            engine.remove_project(&arg::<String>(args, "projectId")?)?;
            Ok(Value::Null)
        }
        "get_project" => {
            let data = engine.get_project(&arg::<String>(args, "projectId")?)?;
            host.allow_directory(Path::new(&data.project.root_path));
            ok(data)
        }

        "start_analysis" => {
            let project_id: String = arg(args, "projectId")?;
            let settings = match arg::<Option<CullSettings>>(args, "settings")? {
                Some(s) => s,
                None => engine.settings.read().cull.clone(),
            };
            engine.start_analysis(&project_id, settings)?;
            Ok(Value::Null)
        }
        "cancel_analysis" => {
            engine.cancel_analysis(&arg::<String>(args, "projectId")?);
            Ok(Value::Null)
        }
        "recompute" => {
            let settings: CullSettings = arg(args, "settings")?;
            ok(engine.recompute(&arg::<String>(args, "projectId")?, settings)?)
        }
        "set_marks" => {
            let ids: Vec<String> = arg(args, "photoIds")?;
            let patch: MarkPatch = arg(args, "patch")?;
            ok(engine.set_marks(&arg::<String>(args, "projectId")?, &ids, patch)?)
        }
        "large_preview" => {
            ok(engine.large_preview(&arg::<String>(args, "projectId")?, &arg::<String>(args, "photoId")?)?)
        }
        "search" => ok(engine.search(&arg::<String>(args, "projectId")?, &arg::<String>(args, "query")?)?),

        "export_project" => {
            let options: ExportOptions = arg(args, "options")?;
            if let Some(dest) = &options.copy_selected.destination {
                host.allow_directory(Path::new(dest));
            }
            ok(engine.export_project(&arg::<String>(args, "projectId")?, &options)?)
        }
        "list_galleries" => ok(engine.list_galleries()?),
        "upload_to_pelli" => {
            let request: UploadRequest = arg(args, "request")?;
            engine.start_upload(&arg::<String>(args, "projectId")?, request)?;
            Ok(Value::Null)
        }
        "cancel_upload" => {
            engine.cancel_upload(&arg::<String>(args, "projectId")?);
            Ok(Value::Null)
        }

        "reveal_in_folder" => {
            host.reveal(Path::new(&arg::<String>(args, "path")?))?;
            Ok(Value::Null)
        }
        "open_external" => {
            let url: String = arg(args, "url")?;
            if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("mailto:")) {
                return Err(EngineError::Remote { code: "invalidArgument".into(), message: "url scheme".into() });
            }
            host.open_external(&url)?;
            Ok(Value::Null)
        }
        other => Err(EngineError::Remote { code: "unknownCommand".into(), message: other.to_string() }),
    }
}

/// `{ "code", "message", "details"? }` error payload of the contract
/// (`details` for the quota errors).
pub fn error_json(e: &EngineError) -> Value {
    let mut v = json!({ "code": e.code(), "message": e.to_string() });
    if let Some(details) = e.details() {
        v["details"] = details;
    }
    v
}
