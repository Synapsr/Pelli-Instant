//! Application settings (persisted in the library's key/value table).

use crate::store::Store;
use crate::{Engine, Result};
use cull_types::{CullSettings, Precision};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Dark by default, like Lightroom and Capture One: colours and exposure
/// are judged on a dark surround (product spec).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    System,
    #[default]
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum AccelerationSetting {
    #[default]
    Auto,
    Cpu,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: Theme,
    /// One of [`LANGUAGES`]; empty until the app picks the computer's
    /// language on first launch.
    pub language: String,
    pub acceleration: AccelerationSetting,
    /// Analyse in the background: a few worker threads at a low priority
    /// (macOS efficiency cores at background QoS, below-normal priority on
    /// Windows) so the computer stays fluid for Lightroom or a browser. Off:
    /// every core, at user-initiated priority (fastest).
    pub background: bool,
    /// Models of the analyses: `Maximum` is kept while the Maximum pack is
    /// missing (analyses then run in Standard).
    pub precision: Precision,
    /// What the culling looks for by default: every project opened for the
    /// first time starts from these (the photographer can still change them
    /// before its analysis, and later in the review).
    pub cull: CullSettings,
    /// Filled when sent to the UI.
    #[serde(default)]
    pub cache_dir: String,
    #[serde(default)]
    pub cache_size_bytes: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            theme: Theme::Dark,
            language: String::new(),
            acceleration: AccelerationSetting::Auto,
            background: false,
            precision: Precision::Standard,
            cull: CullSettings::default(),
            cache_dir: String::new(),
            cache_size_bytes: 0,
        }
    }
}

/// Patch sent by `setSettings` (every field optional).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub theme: Option<Theme>,
    pub language: Option<String>,
    pub acceleration: Option<AccelerationSetting>,
    pub background: Option<bool>,
    pub precision: Option<Precision>,
    /// Replaces the default culling criteria.
    pub cull: Option<CullSettings>,
}

const KEY: &str = "app_settings";
/// The app's languages (Pelli's).
pub const LANGUAGES: &[&str] = &["en", "fr", "es", "de", "it", "pt", "nl", "pl", "sv"];

impl AppSettings {
    pub(crate) fn load(store: &Store, cache_dir: &Path) -> AppSettings {
        let mut s: AppSettings =
            store.kv_get(KEY).ok().flatten().and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default();
        s.cache_dir = cache_dir.to_string_lossy().into_owned();
        s
    }

    /// Analysis worker threads: every core, or the background count
    /// ([`crate::system::background_threads`]).
    pub fn worker_threads(&self) -> usize {
        if self.background {
            crate::system::background_threads()
        } else {
            crate::system::cpu_cores().clamp(1, 32)
        }
    }
}

impl Engine {
    /// The app language: the settings' one, English until the app picked
    /// one on first launch.
    pub fn language(&self) -> String {
        let l = self.settings.read().language.clone();
        if LANGUAGES.contains(&l.as_str()) {
            l
        } else {
            "en".into()
        }
    }

    pub fn get_settings(&self) -> AppSettings {
        let mut s = self.settings.read().clone();
        s.cache_size_bytes = self.cache_size();
        s
    }

    pub fn set_settings(&self, patch: SettingsPatch) -> Result<AppSettings> {
        let reload_models;
        {
            let mut s = self.settings.write();
            if let Some(t) = patch.theme {
                s.theme = t;
            }
            if let Some(l) = patch.language.filter(|l| LANGUAGES.contains(&l.as_str())) {
                s.language = l;
            }
            reload_models = patch.acceleration.is_some_and(|a| a != s.acceleration);
            if let Some(a) = patch.acceleration {
                s.acceleration = a;
            }
            if let Some(b) = patch.background {
                s.background = b;
            }
            if let Some(p) = patch.precision {
                s.precision = p;
            }
            if let Some(mut c) = patch.cull {
                c.keep_per_group = c.keep_per_group.clamp(1, 5);
                s.cull = c;
            }
            let mut stored = s.clone();
            stored.cache_size_bytes = 0;
            self.store.lock().kv_set(KEY, &serde_json::to_string(&stored)?)?;
        }
        if reload_models {
            self.reload_models();
        }
        Ok(self.get_settings())
    }

    /// Small persistent values for hosts (account token, window state…).
    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        self.store.lock().kv_get(key)
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.store.lock().kv_set(key, value)
    }

    pub fn kv_delete(&self, key: &str) -> Result<()> {
        self.store.lock().kv_delete(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_workers() {
        let d = AppSettings::default();
        assert!(!d.background && d.precision == Precision::Standard);
        assert_eq!(d.worker_threads(), crate::system::cpu_cores().clamp(1, 32));
        let bg = AppSettings { background: true, ..AppSettings::default() };
        assert_eq!(bg.worker_threads(), crate::system::background_threads());
        let json = serde_json::to_value(AppSettings { precision: Precision::Maximum, ..d }).unwrap();
        assert_eq!((json["background"].clone(), json["precision"].clone()), (false.into(), "maximum".into()));
        assert!(json.get("threads").is_none());
        let p: SettingsPatch = serde_json::from_str(r#"{"precision":"maximum","background":true}"#).unwrap();
        assert_eq!((p.precision, p.background), (Some(Precision::Maximum), Some(true)));
    }
}
