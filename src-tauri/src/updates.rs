//! In-app updates, through Tauri's updater plugin.
//!
//! A release publishes `latest.json` next to bundles signed with the updater
//! key (`plugins.updater` in `tauri.conf.json`). Pelli Instant reads it a few
//! seconds after launch and every six hours, without a word unless there is
//! something new: then the window offers it. Installing downloads the update,
//! checks its signature, waits until no job the photographer started is under
//! way ([`Engine::busy`]: analysis, export, upload, Maximum pack) and restarts
//! the app on the new version.
//!
//! Only official release builds look for updates: not debug builds, nor
//! builds without the Pelli account (`PELLI_INSTANT_NO_ACCOUNT`).
//! `PELLI_INSTANT_DISABLE_UPDATER=1` turns them off, for packagers that update
//! the app themselves.
//!
//! [`Engine::busy`]: cull_engine::Engine::busy

use crate::AppState;
use serde::Serialize;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Error, Update, UpdaterExt};

/// Leaves launch to the things that matter first.
const FIRST_CHECK: Duration = Duration::from_secs(8);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// How often a downloaded update looks for the end of the jobs under way.
const BUSY_POLL: Duration = Duration::from_secs(1);
/// Progress events, at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(120);

/// Where updates stand (`UpdateStatus` in the contract), sent to the window
/// as `update-status` events.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum UpdateStatus {
    /// Not checked yet.
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
    },
    Downloading {
        version: String,
        /// From 0 to 1, when the size is known.
        progress: Option<f64>,
    },
    /// Downloaded and verified: installs and restarts as soon as no job is
    /// under way (`waiting` while one is).
    Restarting {
        version: String,
        waiting: bool,
    },
    Failed {
        /// `offline`, `unsupported`, `signature` or `internal`.
        code: &'static str,
        message: String,
        /// The update that failed to install, which can be tried again;
        /// `None` when checking failed.
        version: Option<String>,
    },
    /// A debug build, a build without the account, or
    /// `PELLI_INSTANT_DISABLE_UPDATER`.
    Disabled,
}

/// Why checking or installing failed, in terms the window can explain.
fn error_code(error: &Error) -> &'static str {
    match error {
        Error::Reqwest(_) | Error::Network(_) | Error::Http(_) | Error::ReleaseNotFound => "offline",
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) | Error::UnsupportedOs | Error::UnsupportedArch => {
            "unsupported"
        }
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => "signature",
        _ => "internal",
    }
}

fn failed(error: &Error, version: Option<String>) -> UpdateStatus {
    UpdateStatus::Failed { code: error_code(error), message: error.to_string(), version }
}

pub struct Updates {
    status: Mutex<UpdateStatus>,
    /// The update found by the last check, ready to install.
    pending: Mutex<Option<Update>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl Updates {
    fn status(&self) -> UpdateStatus {
        lock(&self.status).clone()
    }

    fn set(&self, app: &AppHandle, status: UpdateStatus) {
        *lock(&self.status) = status.clone();
        if let Err(e) = app.emit("update-status", status) {
            log::warn!("event update-status not delivered: {e}");
        }
    }
}

/// Whether `PELLI_INSTANT_DISABLE_UPDATER` asks for no updates.
fn disabled_by(value: Option<&str>) -> bool {
    value
        .map(|v| v.trim().to_ascii_lowercase())
        .is_some_and(|v| !v.is_empty() && !matches!(v.as_str(), "0" | "false" | "no" | "off"))
}

/// The share of a download done, when its size is known.
fn progress(downloaded: u64, total: Option<u64>) -> Option<f64> {
    total.filter(|total| *total > 0).map(|total| (downloaded as f64 / total as f64).min(1.0))
}

/// Keeps track of updates and, in release builds, checks for them in the
/// background.
pub fn start(app: &AppHandle) {
    let switched_off = disabled_by(std::env::var("PELLI_INSTANT_DISABLE_UPDATER").ok().as_deref());
    // A build without the account is someone's own build: never replace it
    // with the official one, which needs a Pelli account.
    let disabled = cfg!(debug_assertions) || switched_off || cull_engine::no_account_build();
    app.manage(Updates {
        status: Mutex::new(if disabled { UpdateStatus::Disabled } else { UpdateStatus::Idle }),
        pending: Mutex::new(None),
    });
    if disabled {
        if switched_off {
            log::info!("Updates are turned off (PELLI_INSTANT_DISABLE_UPDATER)");
        }
        return;
    }
    let app = app.clone();
    let spawned = thread::Builder::new().name("updates".into()).spawn(move || {
        thread::sleep(FIRST_CHECK);
        loop {
            tauri::async_runtime::block_on(check(&app, false));
            thread::sleep(CHECK_EVERY);
        }
    });
    if let Err(e) = spawned {
        log::warn!("Could not schedule update checks: {e}");
    }
}

/// Asks whether a newer Pelli Instant is published. A check asked from the
/// window shows its outcome, failures included; a background one only
/// reports news, so an offline computer hears nothing.
async fn check(app: &AppHandle, manual: bool) -> UpdateStatus {
    let updates = app.state::<Updates>();
    let current = updates.status();
    if matches!(current, UpdateStatus::Disabled | UpdateStatus::Downloading { .. } | UpdateStatus::Restarting { .. }) {
        return current;
    }
    if manual {
        updates.set(app, UpdateStatus::Checking);
    }
    let found = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    let next = match found {
        Ok(Some(update)) => {
            log::info!("Pelli Instant {} is available", update.version);
            let version = update.version.clone();
            *lock(&updates.pending) = Some(update);
            UpdateStatus::Available { version }
        }
        Ok(None) => {
            *lock(&updates.pending) = None;
            UpdateStatus::UpToDate
        }
        Err(e) => {
            log::warn!("Update check failed: {e}");
            if !manual {
                return current;
            }
            failed(&e, None)
        }
    };
    updates.set(app, next.clone());
    next
}

/// Downloads `update`, installs it once no job is under way, and restarts
/// the app. A failure keeps the update, to be tried again.
async fn install(app: &AppHandle, update: Update) {
    let updates = app.state::<Updates>();
    let version = update.version.clone();
    updates.set(app, UpdateStatus::Downloading { version: version.clone(), progress: Some(0.0) });

    let mut downloaded = 0u64;
    let mut last_report = Instant::now();
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                if last_report.elapsed() >= PROGRESS_EVERY {
                    last_report = Instant::now();
                    updates.set(
                        app,
                        UpdateStatus::Downloading { version: version.clone(), progress: progress(downloaded, total) },
                    );
                }
            },
            || {},
        )
        .await;
    let bytes = match bytes {
        Ok(bytes) => bytes,
        Err(e) => return fail(app, &updates, update, &e),
    };

    // Never in the middle of an analysis, an export, an upload or the
    // Maximum pack download.
    let mut waiting = false;
    while busy(app) {
        if !waiting {
            waiting = true;
            log::info!("Pelli Instant {version} will install once the work under way is done");
            updates.set(app, UpdateStatus::Restarting { version: version.clone(), waiting: true });
        }
        pause(BUSY_POLL).await;
    }
    updates.set(app, UpdateStatus::Restarting { version: version.clone(), waiting: false });
    if let Err(e) = update.install(&bytes) {
        return fail(app, &updates, update, &e);
    }
    log::info!("Installed Pelli Instant {version}; restarting");
    app.restart();
}

/// Reports a failed install, keeping the update so it can be tried again.
fn fail(app: &AppHandle, updates: &Updates, update: Update, error: &Error) {
    log::warn!("Update to {} failed: {error}", update.version);
    let status = failed(error, Some(update.version.clone()));
    *lock(&updates.pending) = Some(update);
    updates.set(app, status);
}

fn busy(app: &AppHandle) -> bool {
    app.state::<AppState>().engine.busy()
}

async fn pause(duration: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || thread::sleep(duration)).await;
}

// -- commands (`update_status`, `check_for_update`, `install_update`) --------

#[tauri::command]
pub fn update_status(updates: tauri::State<Updates>) -> UpdateStatus {
    updates.status()
}

#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> UpdateStatus {
    check(&app, true).await
}

/// Starts installing the update found by the last check, in the background
/// (progress and failures as `update-status` events). Nothing to do without
/// one, or while it is already installing.
#[tauri::command]
pub fn install_update(app: AppHandle) {
    let Some(update) = lock(&app.state::<Updates>().pending).take() else { return };
    tauri::async_runtime::spawn(async move { install(&app, update).await });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_turns_updates_off() {
        for on in ["1", "true", "yes", " TRUE "] {
            assert!(disabled_by(Some(on)), "{on}");
        }
        for off in ["", "0", "false", "No", "off"] {
            assert!(!disabled_by(Some(off)), "{off:?}");
        }
        assert!(!disabled_by(None));
    }

    #[test]
    fn progress_needs_a_known_size() {
        assert_eq!(progress(50, Some(200)), Some(0.25));
        assert_eq!(progress(300, Some(200)), Some(1.0));
        assert_eq!(progress(50, None), None);
        assert_eq!(progress(50, Some(0)), None);
    }

    #[test]
    fn errors_are_sorted_for_the_window() {
        assert_eq!(error_code(&Error::Network("unreachable".into())), "offline");
        assert_eq!(error_code(&Error::ReleaseNotFound), "offline");
        assert_eq!(error_code(&Error::TargetNotFound("darwin-x86_64".into())), "unsupported");
        assert_eq!(error_code(&Error::MissingSignedVersion), "signature");
        assert_eq!(error_code(&Error::BinaryNotFoundInArchive), "internal");
    }

    /// The shape the release workflow writes to `latest.json`.
    const LATEST: &str = r#"{
        "version": "0.2.0",
        "notes": "https://github.com/Synapsr/Pelli-Instant/releases/tag/v0.2.0",
        "pub_date": "2026-11-07T10:00:00Z",
        "platforms": {
            "darwin-aarch64": {"signature": "c2lnLW1hYw==", "url": "https://github.com/Synapsr/Pelli-Instant/releases/download/v0.2.0/PelliInstant-macos-arm64.app.tar.gz"},
            "windows-x86_64-nsis": {"signature": "c2lnLW5zaXM=", "url": "https://github.com/Synapsr/Pelli-Instant/releases/download/v0.2.0/PelliInstant-windows-x64-setup.exe"}
        }
    }"#;

    #[test]
    fn the_release_manifest_reads_as_the_plugin_expects() {
        let release: tauri_plugin_updater::RemoteRelease = serde_json::from_str(LATEST).unwrap();
        assert_eq!(release.version.to_string(), "0.2.0");
        assert!(release.download_url("darwin-aarch64").unwrap().as_str().ends_with(".app.tar.gz"));
        assert_eq!(release.signature("windows-x86_64-nsis").unwrap(), "c2lnLW5zaXM=");
        // No Intel Mac build: such a Mac is told, not offered a wrong bundle.
        assert!(release.download_url("darwin-x86_64").is_err());
    }

    #[test]
    fn statuses_read_as_the_contract() {
        let json =
            serde_json::to_value(UpdateStatus::Downloading { version: "0.2.0".into(), progress: Some(0.5) }).unwrap();
        assert_eq!(json, serde_json::json!({ "status": "downloading", "version": "0.2.0", "progress": 0.5 }));
        let json = serde_json::to_value(UpdateStatus::UpToDate).unwrap();
        assert_eq!(json, serde_json::json!({ "status": "upToDate" }));
        let error = Error::Network("unreachable".into());
        let json = serde_json::to_value(failed(&error, None)).unwrap();
        assert_eq!(json["status"], "failed");
        assert_eq!(json["code"], "offline");
        assert_eq!(json["version"], serde_json::Value::Null);
    }
}
