//! Usage statistics: opt-out (`telemetry` setting, on by default),
//! anonymous in the free version, linked to Plus when Plus is active.
//!
//! A few events tell Pelli how the app is used:
//!
//! | Event | When | `props` |
//! | --- | --- | --- |
//! | `onboarding_completed` | the welcome tour is finished (`onboarded` set) | — |
//! | `app_opened` | at start, at most once a calendar day (UTC) | — |
//! | `analysis_completed` | an analysis of the app analysed ≥ 1 photo | `photos`, `raw`, `jpeg` (this run), `seconds`, `precision`, `selected`, `toReview`, `rejected`, `groups` (the project) |
//! | `export_completed` | an export finished | `photos`, `xmp`, `copied`, `moved` (what was asked) |
//! | `plus_activated` | Plus just became active | `via`: `purchase`, `key`, `account` |
//! | `telemetry_disabled` | telemetry was turned off: the last one | — |
//!
//! Each payload holds a random install id (a UUID kept in the library only
//! while telemetry is on, deleted when it is turned off, a new one when it
//! is turned back on), the app version, the platform, the processor
//! architecture and the app language. Never a path or a file name. With Plus
//! active, each event also says whose Plus it is: the licence (its signed
//! token, `license` in the payload) or the Pelli account (its token, in the
//! `Authorization` header, as every account request); Pelli then names the
//! licence or the account in its own notes. The free version sends neither:
//! its statistics stay anonymous. Events are sent fire-and-forget, each
//! on its own background thread (`POST /api/cull/telemetry`, 5 s timeout, no
//! retry, nothing kept on disk): telemetry never slows down nor fails
//! anything.
//!
//! Nothing is sent until the host starts the engine's services
//! ([`Engine::start_services`]): the CLI and the tests never do. Debug builds
//! send only to a Pelli named by `PELLI_API_URL`, never to production, and
//! log every event instead.

use crate::Engine;
use parking_lot::RwLock;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// Every event, in the order of the table above.
pub const EVENTS: &[&str] = &[
    "onboarding_completed",
    "app_opened",
    "analysis_completed",
    "export_completed",
    "plus_activated",
    "telemetry_disabled",
];

/// kv keys.
const INSTALL_ID_KEY: &str = "telemetry.install_id";
/// UTC day number of the last `app_opened`.
const APP_OPENED_KEY: &str = "telemetry.app_opened";
const DAY_MS: i64 = 86_400_000;
const TIMEOUT: Duration = Duration::from_secs(5);

/// Delivers telemetry payloads; must return at once. `bearer`: the Pelli
/// account's token, when the account is what gives Plus.
pub trait TelemetrySender: Send + Sync {
    fn send(&self, payload: Value, bearer: Option<String>);
}

/// `POST <Pelli>/api/cull/telemetry`.
struct HttpSender {
    url: String,
}

impl TelemetrySender for HttpSender {
    fn send(&self, payload: Value, bearer: Option<String>) {
        let url = self.url.clone();
        let job = move || {
            let lang = payload["locale"].as_str().unwrap_or("en").to_string();
            let mut req = crate::pelli::agent(TIMEOUT).post(&url).header("Accept-Language", &lang);
            if let Some(token) = bearer {
                req = req.header("Authorization", format!("Bearer {token}"));
            }
            match req.send_json(&payload) {
                Ok(r) if r.status().is_success() => {}
                Ok(r) => log::debug!("telemetry {}: HTTP {}", payload["event"], r.status()),
                Err(e) => log::debug!("telemetry {}: {e}", payload["event"]),
            }
        };
        if let Err(e) = std::thread::Builder::new().name("telemetry".into()).spawn(job) {
            log::debug!("telemetry not sent: {e}");
        }
    }
}

/// A debug build talking to the production Pelli: log only.
struct LogOnly;

impl TelemetrySender for LogOnly {
    fn send(&self, payload: Value, _bearer: Option<String>) {
        log::info!("telemetry (a debug build sends it only to PELLI_API_URL): {payload}");
    }
}

/// The engine's sender: none until the host starts the services.
#[derive(Default)]
pub(crate) struct Telemetry {
    sender: RwLock<Option<Arc<dyn TelemetrySender>>>,
}

/// `arm64` or `x64` (the architecture's Rust name otherwise).
pub fn arch() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    }
}

/// What "What is sent?" shows (`telemetry_preview`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryPreview {
    /// `None` while telemetry is off.
    pub install_id: Option<String>,
    pub events: Vec<&'static str>,
    /// An `analysis_completed` payload as it would be sent, pretty-printed.
    pub example: String,
}

impl Engine {
    /// Where telemetry goes instead of Pelli (tests, another host).
    pub fn set_telemetry_sender(&self, sender: Arc<dyn TelemetrySender>) {
        *self.telemetry.sender.write() = Some(sender);
    }

    /// Sends to Pelli from now on (unless another sender is set).
    pub(crate) fn start_telemetry(&self) {
        let mut slot = self.telemetry.sender.write();
        if slot.is_none() {
            let to_production = !crate::pelli::api_url_configured();
            *slot = Some(if cfg!(debug_assertions) && to_production {
                Arc::new(LogOnly)
            } else {
                Arc::new(HttpSender { url: format!("{}/api/cull/telemetry", self.api_url.read()) })
            });
        }
    }

    /// The install id, created when missing and `create` is set.
    fn install_id(&self, create: bool) -> Option<String> {
        if let Some(id) = self.kv_get(INSTALL_ID_KEY).ok().flatten() {
            return Some(id);
        }
        if !create {
            return None;
        }
        let id = crate::plus::random_uuid();
        match self.kv_set(INSTALL_ID_KEY, &id) {
            Ok(()) => Some(id),
            Err(e) => {
                log::warn!("telemetry install id not saved: {e}");
                None
            }
        }
    }

    fn telemetry_payload(&self, install_id: &str, event: &str, props: Value) -> Value {
        json!({
            "installId": install_id,
            "event": event,
            "appVersion": env!("CARGO_PKG_VERSION"),
            "platform": crate::pelli::os(),
            "arch": arch(),
            "locale": self.language(),
            "props": props,
        })
    }

    /// Sends `event` when telemetry is on. `true` when it went out.
    pub(crate) fn track(&self, event: &str, props: Value) -> bool {
        self.settings.read().telemetry && self.send_event(event, props)
    }

    fn send_event(&self, event: &str, props: Value) -> bool {
        let Some(sender) = self.telemetry.sender.read().clone() else { return false };
        let Some(id) = self.install_id(true) else { return false };
        let mut payload = self.telemetry_payload(&id, event, props);
        let (license, bearer) = self.plus_identity();
        if let Some(token) = license {
            payload["license"] = Value::String(token);
        }
        sender.send(payload, bearer);
        true
    }

    /// Whose Plus it is, for the statistics: the licence token, or the
    /// account token when the account gives Plus. Neither without Plus.
    fn plus_identity(&self) -> (Option<String>, Option<String>) {
        if let Some(license) = self.valid_license() {
            return (Some(license.token), None);
        }
        if self.account_plus() {
            return (None, self.token());
        }
        (None, None)
    }

    /// `app_opened`, at most once a calendar day, once the welcome tour is
    /// done.
    pub(crate) fn app_opened(&self) {
        if !self.settings.read().onboarded {
            return;
        }
        let today = crate::now_ms().div_euclid(DAY_MS).to_string();
        if self.kv_get(APP_OPENED_KEY).ok().flatten().as_deref() == Some(today.as_str()) {
            return;
        }
        if self.track("app_opened", json!({})) {
            if let Err(e) = self.kv_set(APP_OPENED_KEY, &today) {
                log::warn!("app_opened not recorded: {e}");
            }
        }
    }

    /// Telemetry was just turned on: a new install id.
    pub(crate) fn telemetry_turned_on(&self) {
        self.install_id(true);
    }

    /// Telemetry was just turned off: one last `telemetry_disabled` when it
    /// had been on after the welcome tour (`notify`), nothing when it is
    /// declined during the tour; then the install id is forgotten.
    pub(crate) fn telemetry_turned_off(&self, notify: bool) {
        if notify {
            self.send_event("telemetry_disabled", json!({}));
        }
        if let Err(e) = self.kv_delete(INSTALL_ID_KEY) {
            log::warn!("telemetry install id not deleted: {e}");
        }
    }

    pub fn telemetry_preview(&self) -> TelemetryPreview {
        let install_id = if self.settings.read().telemetry { self.install_id(true) } else { None };
        let example = self.telemetry_payload(
            install_id.as_deref().unwrap_or("00000000-0000-4000-8000-000000000000"),
            "analysis_completed",
            json!({ "photos": 854, "raw": 854, "jpeg": 0, "seconds": 10.2, "precision": "standard",
                    "selected": 512, "toReview": 201, "rejected": 141, "groups": 97 }),
        );
        TelemetryPreview {
            install_id,
            events: EVENTS.to_vec(),
            example: serde_json::to_string_pretty(&example).unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::settings::SettingsPatch;
    use crate::testing::{test_engine, TestSender};
    use crate::NullSink;
    use serde_json::json;
    use std::sync::Arc;

    fn engine(dir: &std::path::Path) -> (Arc<crate::Engine>, Arc<TestSender>) {
        let engine = test_engine(dir, Arc::new(NullSink));
        let sender = Arc::new(TestSender::default());
        engine.set_telemetry_sender(sender.clone());
        (engine, sender)
    }

    fn patch(f: impl FnOnce(&mut SettingsPatch)) -> SettingsPatch {
        let mut p = SettingsPatch::default();
        f(&mut p);
        p
    }

    #[test]
    fn nothing_is_sent_without_a_sender() {
        let tmp = tempfile::tempdir().unwrap();
        let engine = test_engine(tmp.path(), Arc::new(NullSink));
        assert!(!engine.track("app_opened", json!({})));
        engine.set_settings(patch(|p| p.onboarded = Some(true))).unwrap();
        assert_eq!(engine.kv_get(super::INSTALL_ID_KEY).unwrap(), None, "no id for nothing");
    }

    #[test]
    fn onboarding_and_daily_opening() {
        let tmp = tempfile::tempdir().unwrap();
        let (engine, sender) = engine(tmp.path());
        // Before the welcome tour is done: nothing.
        engine.app_opened();
        assert!(sender.events().is_empty());
        engine.set_settings(patch(|p| p.onboarded = Some(true))).unwrap();
        engine.set_settings(patch(|p| p.onboarded = Some(true))).unwrap();
        assert_eq!(sender.events(), ["onboarding_completed"], "the first time only");
        engine.app_opened();
        engine.app_opened();
        assert_eq!(sender.events(), ["onboarding_completed", "app_opened"], "once a day");

        let p = &sender.payloads()[1];
        let id = p["installId"].as_str().unwrap();
        assert_eq!((id.len(), &id[14..15]), (36, "4"), "UUID v4");
        assert_eq!(p["event"], "app_opened");
        assert_eq!(p["appVersion"], env!("CARGO_PKG_VERSION"));
        assert_eq!(p["platform"], crate::pelli::os());
        assert!(p["arch"] == "arm64" || p["arch"] == "x64");
        assert_eq!(p["locale"], "en");
        assert_eq!(p["props"], json!({}));
        let keys: Vec<&String> = p.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 7, "nothing else: {keys:?}");
        assert_eq!(sender.payloads()[0]["installId"], p["installId"]);

        // Yesterday's opening was another day.
        engine.kv_set(super::APP_OPENED_KEY, "1").unwrap();
        engine.set_settings(patch(|p| p.language = Some("fr".into()))).unwrap();
        engine.app_opened();
        assert_eq!(sender.payloads().last().unwrap()["locale"], "fr");
    }

    #[test]
    fn turning_it_off_sends_one_last_event_and_forgets_the_id() {
        let tmp = tempfile::tempdir().unwrap();
        let (engine, sender) = engine(tmp.path());
        engine.set_settings(patch(|p| p.onboarded = Some(true))).unwrap();
        let first = engine.telemetry_preview().install_id.unwrap();
        assert_eq!(sender.payloads()[0]["installId"], first.as_str());

        engine.set_settings(patch(|p| p.telemetry = Some(false))).unwrap();
        assert_eq!(sender.events(), ["onboarding_completed", "telemetry_disabled"]);
        assert_eq!(sender.payloads()[1]["installId"], first.as_str());
        assert_eq!(engine.kv_get(super::INSTALL_ID_KEY).unwrap(), None);
        // Then nothing at all.
        engine.kv_delete(super::APP_OPENED_KEY).unwrap();
        engine.app_opened();
        assert!(!engine.track("export_completed", json!({})));
        engine.set_settings(patch(|p| p.telemetry = Some(false))).unwrap();
        assert_eq!(sender.events().len(), 2);
        let preview = engine.telemetry_preview();
        assert_eq!(preview.install_id, None);
        assert_eq!(engine.kv_get(super::INSTALL_ID_KEY).unwrap(), None, "the preview creates none");

        // Back on: a new id.
        engine.set_settings(patch(|p| p.telemetry = Some(true))).unwrap();
        let second = engine.telemetry_preview().install_id.unwrap();
        assert_ne!(second, first);
        engine.app_opened();
        assert_eq!(sender.payloads().last().unwrap()["installId"], second.as_str());
    }

    #[test]
    fn turned_off_during_onboarding() {
        let tmp = tempfile::tempdir().unwrap();
        let (engine, sender) = engine(tmp.path());
        engine
            .set_settings(patch(|p| {
                p.telemetry = Some(false);
                p.onboarded = Some(true);
            }))
            .unwrap();
        // Declined before the first use: nothing is ever sent.
        assert!(sender.events().is_empty());
        assert_eq!(engine.kv_get(super::INSTALL_ID_KEY).unwrap(), None);
    }

    #[test]
    fn preview_lists_every_event() {
        let tmp = tempfile::tempdir().unwrap();
        let (engine, sender) = engine(tmp.path());
        let preview = engine.telemetry_preview();
        let v = serde_json::to_value(&preview).unwrap();
        assert_eq!(v["events"], json!(super::EVENTS));
        assert_eq!(v["installId"], json!(preview.install_id));
        let example: serde_json::Value = serde_json::from_str(&preview.example).unwrap();
        assert_eq!(example["event"], "analysis_completed");
        assert_eq!(example["installId"], json!(preview.install_id));
        assert!(sender.events().is_empty(), "a preview sends nothing");
    }

    #[test]
    fn plus_activation_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let (engine, sender) = engine(tmp.path());
        let before = engine.plus_status();
        engine.kv_set("pelli.token", "t").unwrap();
        let account = crate::pelli::Account { plus: true, ..Default::default() };
        engine.kv_set("pelli.account", &serde_json::to_string(&account).unwrap()).unwrap();
        engine.plus_changed(&before, crate::plus::Via::Account);
        assert_eq!(sender.payloads()[0]["event"], "plus_activated");
        assert_eq!(sender.payloads()[0]["props"], json!({ "via": "account" }));
    }

    /// The app's analyses and exports are reported; the CLI's batch
    /// analysis is not, nor a run that analysed nothing.
    #[test]
    fn analyses_and_exports_are_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let events = Arc::new(crate::testing::Recorder::default());
        let engine = test_engine(tmp.path(), events.clone());
        let sender = Arc::new(TestSender::default());
        engine.set_telemetry_sender(sender.clone());
        let shoot = tmp.path().join("shoot");
        std::fs::create_dir_all(&shoot).unwrap();
        for (i, shade) in [40u8, 120, 200].iter().enumerate() {
            let img = image::RgbImage::from_fn(320, 240, |x, y| image::Rgb([*shade, (x % 255) as u8, (y % 255) as u8]));
            std::fs::write(shoot.join(format!("IMG_{i}.jpg")), cull_decode::encode_jpeg(&img, 85)).unwrap();
        }
        let project = engine.open_folder(&shoot, false).unwrap();
        let settings = engine.get_settings().cull;
        let run = |n: usize| {
            engine.start_analysis(&project.id, settings.clone()).unwrap();
            let t = std::time::Instant::now();
            while events.named("analysis-done").len() < n && t.elapsed() < std::time::Duration::from_secs(60) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            while engine.is_running(&project.id) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        run(1);
        assert_eq!(sender.events(), ["analysis_completed"]);
        let props = sender.payloads()[0]["props"].clone();
        assert_eq!(
            (props["photos"].clone(), props["raw"].clone(), props["jpeg"].clone()),
            (json!(3), json!(0), json!(3))
        );
        assert_eq!(props["precision"], "standard");
        let sorted = ["selected", "toReview", "rejected"].iter().map(|k| props[k].as_u64().unwrap()).sum::<u64>();
        assert_eq!(sorted, 3);
        assert!(props["seconds"].as_f64().unwrap() >= 0.0 && props["groups"].is_u64());
        let keys: Vec<&String> = props.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 9, "{keys:?}");

        // Nothing new to analyse: nothing reported.
        run(2);
        assert_eq!(sender.events().len(), 1);
        // The CLI's batch analysis never reports.
        engine.remove_project(&project.id).unwrap();
        let project = engine.open_folder(&shoot, false).unwrap();
        engine.analyze_blocking(&project.id, settings.clone()).unwrap();
        assert_eq!(sender.events().len(), 1);

        let options: crate::view::ExportOptions = serde_json::from_value(json!({
            "xmp": { "enabled": true, "rating": { "selected": 3, "toReview": 2, "rejected": -1 },
                     "label": { "selected": "green", "toReview": "none", "rejected": "red" }, "keepUserMarks": true },
            "copySelected": { "enabled": false, "destination": null, "includeRaw": true, "includeJpeg": true,
                              "keepStructure": false },
            "moveRejected": { "enabled": false, "folderName": "_Rejets Pelli" },
            "fileList": false
        }))
        .unwrap();
        engine.export_project(&project.id, &options).unwrap();
        assert_eq!(sender.events(), ["analysis_completed", "export_completed"]);
        assert_eq!(sender.payloads()[1]["props"], json!({ "photos": 3, "xmp": true, "copied": false, "moved": false }));
        let text = sender.payloads().iter().map(|p| p.to_string()).collect::<String>();
        assert!(!text.contains("IMG_") && !text.contains("shoot"), "no file name nor path: {text}");
    }
}
