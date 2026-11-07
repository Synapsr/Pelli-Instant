//! Monthly analysis quota.
//!
//! Pelli counts the photos sorted per calendar month (UTC); how many depends
//! on the Pelli plan, some plans are unlimited (`docs/pelli-api.md`). A photo
//! counts once, the first time it is analysed in a project: analysing it
//! again (changed file, new models, previous error) never counts, and a RAW +
//! JPEG pair is one photo.
//!
//! Before analysing, the engine reserves the photos it is about to analyse
//! for the first time (`usage/reserve`, all or nothing). When the run ends,
//! those it did not analyse after all (cancelled run, unreadable files) are
//! given back (`usage/release`). The photos of a run on a limited quota are
//! kept in the library until it ends: after a quit or a crash, the next
//! analysis of the project does not reserve them again and gives back those
//! it no longer needs. Offline, an unlimited plan analyses without
//! counting; a limited one cannot (`quotaOffline`). The CLI and the
//! no-account build ([`crate::no_account_build`]) never count anything.

use crate::account::signs_out;
use crate::pelli::{self, Entitlements, PelliClient, QuotaUsage, Reserve};
use crate::{Engine, EngineError, Result};
use serde::Serialize;
use std::collections::HashSet;

/// kv key, followed by `.<project id>`: ids of the photos a run reserved on
/// a limited quota, until it ends.
const RESERVED_KEY: &str = "usage.reserved";

/// Largest `photos` of one `usage/reserve` call (Pelli's bound). No limited
/// plan comes close: a bigger analysis asks for this many, which an
/// unlimited plan grants and a limited one refuses.
const MAX_PHOTOS_PER_REQUEST: u32 = 50_000;

/// Details of the `quotaExceeded` and `quotaOffline` errors (`QuotaDetails`
/// in the UI contract).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaDetails {
    /// Photos per month; `None` = unlimited (or unknown when offline).
    pub monthly_quota: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_this_month: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining: Option<u32>,
    /// First instant of next month (UTC, epoch ms).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<i64>,
    /// Photos this analysis needed.
    pub needed: u32,
}

/// The Pelli calls behind the quota: [`PelliClient`], or a fake in tests.
pub(crate) trait UsageApi {
    fn reserve(&self, photos: u32) -> Result<Reserve>;
    /// Returns the new `usedThisMonth`.
    fn release(&self, photos: u32) -> Result<u32>;
}

impl UsageApi for PelliClient {
    fn reserve(&self, photos: u32) -> Result<Reserve> {
        self.reserve_usage(photos)
    }

    fn release(&self, photos: u32) -> Result<u32> {
        self.release_usage(photos)
    }
}

/// How an analysis was counted.
#[derive(Debug, PartialEq)]
pub(crate) enum Counted {
    /// On a limited quota: what is not analysed in the end is given back.
    Limited(QuotaUsage),
    /// On an unlimited plan: nothing to give back.
    Unlimited(QuotaUsage),
    /// Not counted: Pelli is unreachable and the cached plan is unlimited.
    Offline,
}

/// Reserves `needed` photos. `cached` (the entitlements of the cached
/// account) only matters when Pelli cannot be reached.
pub(crate) fn reserve(api: &dyn UsageApi, needed: u32, cached: Option<&Entitlements>) -> Result<Counted> {
    match api.reserve(needed.min(MAX_PHOTOS_PER_REQUEST)) {
        Ok(Reserve::Granted { unlimited: true, usage }) => Ok(Counted::Unlimited(usage)),
        Ok(Reserve::Granted { unlimited: false, usage }) => Ok(Counted::Limited(usage)),
        Ok(Reserve::Refused(usage)) => Err(EngineError::QuotaExceeded(QuotaDetails {
            monthly_quota: usage.monthly_quota,
            used_this_month: Some(usage.used_this_month),
            remaining: usage.remaining,
            resets_at: Some(usage.resets_at),
            needed,
        })),
        Err(e) if pelli::unreachable(&e) => match cached {
            Some(ent) if ent.monthly_quota.is_none() => Ok(Counted::Offline),
            _ => Err(EngineError::QuotaOffline(QuotaDetails {
                monthly_quota: cached.and_then(|ent| ent.monthly_quota),
                used_this_month: None,
                remaining: None,
                resets_at: None,
                needed,
            })),
        },
        Err(e) => Err(e),
    }
}

/// First instant of the calendar month after `ms` (both epoch ms, UTC): when
/// Pelli resets the monthly count.
pub fn next_month_utc(ms: i64) -> i64 {
    const DAY_MS: i64 = 86_400_000;
    // Civil date ↔ day number (proleptic Gregorian, H. Hinnant's algorithms).
    let days = ms.div_euclid(DAY_MS);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let (year, month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    // Day number of `year-month-01`.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe - 719_468) * DAY_MS
}

impl Engine {
    /// Counts the `needed` photos an analysis sorts for the first time.
    /// `Ok(true)` when they were counted on a limited quota: the run gives
    /// back what it did not analyse with [`Engine::release_usage`].
    pub(crate) fn reserve_usage(&self, needed: u32) -> Result<bool> {
        if needed == 0 {
            return Ok(false);
        }
        let client = self.signed_in_client()?;
        self.reserve_with(&client, needed)
    }

    fn reserve_with(&self, api: &dyn UsageApi, needed: u32) -> Result<bool> {
        let cached = self.cached_account();
        match reserve(api, needed, cached.as_ref().map(|a| &a.entitlements)) {
            Ok(Counted::Limited(usage)) => {
                self.record_usage(&usage);
                Ok(true)
            }
            Ok(Counted::Unlimited(usage)) => {
                self.record_usage(&usage);
                Ok(false)
            }
            Ok(Counted::Offline) => {
                log::warn!("Pelli unreachable: {needed} photo(s) analysed without counting (unlimited plan)");
                Ok(false)
            }
            Err(EngineError::QuotaExceeded(details)) => {
                if let (Some(used), Some(resets_at)) = (details.used_this_month, details.resets_at) {
                    self.update_entitlements(|e| {
                        e.monthly_quota = details.monthly_quota;
                        e.used_this_month = used;
                        e.resets_at = resets_at;
                    });
                }
                Err(EngineError::QuotaExceeded(details))
            }
            Err(e) => {
                if signs_out(&e) {
                    if let Err(logout) = self.auth_logout() {
                        log::warn!("sign-out after a refused token failed: {logout}");
                    }
                }
                Err(e)
            }
        }
    }

    /// Gives back `photos` reserved but not analysed. Best effort: a failure
    /// is only logged (the photos stay counted).
    pub(crate) fn release_usage(&self, photos: u32) {
        if photos == 0 {
            return;
        }
        match self.signed_in_client() {
            Ok(client) => self.release_with(&client, photos),
            Err(e) => log::warn!("cannot give back {photos} unused photo(s): {e}"),
        }
    }

    fn release_with(&self, api: &dyn UsageApi, photos: u32) {
        match api.release(photos) {
            Ok(used) => self.update_entitlements(|e| e.used_this_month = used),
            Err(e) => log::warn!("cannot give back {photos} unused photo(s): {e}"),
        }
    }

    /// Photos of `project_id` reserved by a run that did not end.
    pub(crate) fn reserved_photos(&self, project_id: &str) -> HashSet<String> {
        self.kv_get(&format!("{RESERVED_KEY}.{project_id}"))
            .ok()
            .flatten()
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default()
    }

    /// Remembers the photos a run has reserved, until
    /// [`Engine::clear_reserved_photos`].
    pub(crate) fn set_reserved_photos(&self, project_id: &str, ids: &[String]) {
        let saved = serde_json::to_string(ids)
            .map_err(EngineError::from)
            .and_then(|j| self.kv_set(&format!("{RESERVED_KEY}.{project_id}"), &j));
        if let Err(e) = saved {
            log::warn!("reserved photos not remembered: {e}");
        }
    }

    pub(crate) fn clear_reserved_photos(&self, project_id: &str) {
        if let Err(e) = self.kv_delete(&format!("{RESERVED_KEY}.{project_id}")) {
            log::warn!("reserved photos not forgotten: {e}");
        }
    }

    /// Mirrors Pelli's count in the cached account (the UI shows it).
    fn record_usage(&self, usage: &QuotaUsage) {
        self.update_entitlements(|e| {
            e.monthly_quota = usage.monthly_quota;
            e.used_this_month = usage.used_this_month;
            e.resets_at = usage.resets_at;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::AuthState;
    use crate::pelli::{Account, Plan};
    use crate::{EngineConfig, EventSink};
    use parking_lot::Mutex;
    use std::path::Path;
    use std::sync::Arc;

    /// Scripted answers of the usage API, and the calls received.
    struct FakeUsage {
        reserve: Mutex<Option<Result<Reserve>>>,
        release: Mutex<Option<Result<u32>>>,
        calls: Mutex<Vec<String>>,
    }

    impl FakeUsage {
        fn reserving(answer: Result<Reserve>) -> FakeUsage {
            FakeUsage { reserve: Mutex::new(Some(answer)), release: Mutex::new(None), calls: Mutex::default() }
        }

        fn releasing(answer: Result<u32>) -> FakeUsage {
            FakeUsage { reserve: Mutex::new(None), release: Mutex::new(Some(answer)), calls: Mutex::default() }
        }
    }

    impl UsageApi for FakeUsage {
        fn reserve(&self, photos: u32) -> Result<Reserve> {
            self.calls.lock().push(format!("reserve {photos}"));
            self.reserve.lock().take().expect("unexpected reserve")
        }

        fn release(&self, photos: u32) -> Result<u32> {
            self.calls.lock().push(format!("release {photos}"));
            self.release.lock().take().expect("unexpected release")
        }
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<(String, serde_json::Value)>>);

    impl EventSink for Recorder {
        fn emit(&self, event: &str, payload: serde_json::Value) {
            self.0.lock().push((event.to_string(), payload));
        }
    }

    fn usage(quota: Option<u32>, used: u32) -> QuotaUsage {
        QuotaUsage {
            monthly_quota: quota,
            used_this_month: used,
            remaining: quota.map(|q| q.saturating_sub(used)),
            resets_at: 1_798_761_600_000,
        }
    }

    fn offline() -> EngineError {
        EngineError::Remote { code: "network".into(), message: "connection refused".into() }
    }

    fn entitlements(quota: Option<u32>) -> Entitlements {
        Entitlements { monthly_quota: quota, ..Default::default() }
    }

    /// An engine signed in with a cached account on `quota`.
    fn signed_in_engine(dir: &Path, quota: Option<u32>) -> (Arc<Engine>, Arc<Recorder>) {
        let events = Arc::new(Recorder::default());
        let engine = Engine::open(EngineConfig {
            data_dir: dir.join("data"),
            cache_dir: dir.join("cache"),
            models_dir: dir.join("no-models"),
            pack_dir: None,
            url_mapper: Arc::new(|p: &Path| p.display().to_string()),
            events: events.clone(),
            secrets: None,
        })
        .unwrap();
        let account = Account { plan: Plan::Free, entitlements: entitlements(quota), ..Default::default() };
        engine.kv_set("pelli.token", "token").unwrap();
        engine.kv_set("pelli.account", &serde_json::to_string(&account).unwrap()).unwrap();
        (engine, events)
    }

    #[test]
    fn granted_reservations() {
        let api = FakeUsage::reserving(Ok(Reserve::Granted { unlimited: false, usage: usage(Some(200), 40) }));
        assert_eq!(reserve(&api, 40, None).unwrap(), Counted::Limited(usage(Some(200), 40)));
        assert_eq!(*api.calls.lock(), ["reserve 40"]);
        let api = FakeUsage::reserving(Ok(Reserve::Granted { unlimited: true, usage: usage(None, 900) }));
        assert_eq!(reserve(&api, 900, None).unwrap(), Counted::Unlimited(usage(None, 900)));
        // Within Pelli's bound per request.
        let api = FakeUsage::reserving(Ok(Reserve::Granted { unlimited: true, usage: usage(None, 0) }));
        reserve(&api, 80_000, None).unwrap();
        assert_eq!(*api.calls.lock(), ["reserve 50000"]);
    }

    #[test]
    fn refused_reservation_carries_the_details() {
        let api = FakeUsage::reserving(Ok(Reserve::Refused(usage(Some(200), 190))));
        let err = reserve(&api, 25, Some(&entitlements(Some(200)))).unwrap_err();
        assert_eq!(err.code(), "quotaExceeded");
        assert_eq!(
            err.details().unwrap(),
            serde_json::json!({ "monthlyQuota": 200, "usedThisMonth": 190, "remaining": 10,
                                "resetsAt": 1_798_761_600_000_i64, "needed": 25 })
        );
    }

    #[test]
    fn offline_reservations() {
        // Unlimited plan: go on without counting.
        let api = FakeUsage::reserving(Err(offline()));
        assert_eq!(reserve(&api, 30, Some(&entitlements(None))).unwrap(), Counted::Offline);
        // Server down is offline too.
        let down = EngineError::Remote { code: "http_502".into(), message: "HTTP 502".into() };
        let api = FakeUsage::reserving(Err(down));
        assert_eq!(reserve(&api, 30, Some(&entitlements(None))).unwrap(), Counted::Offline);
        // Limited plan, or nothing known: the photos cannot be counted.
        let api = FakeUsage::reserving(Err(offline()));
        let err = reserve(&api, 30, Some(&entitlements(Some(1000)))).unwrap_err();
        assert_eq!(err.code(), "quotaOffline");
        assert_eq!(err.details().unwrap(), serde_json::json!({ "monthlyQuota": 1000, "needed": 30 }));
        let api = FakeUsage::reserving(Err(offline()));
        assert_eq!(reserve(&api, 30, None).unwrap_err().code(), "quotaOffline");
    }

    #[test]
    fn other_errors_pass_through() {
        let api = FakeUsage::reserving(Err(EngineError::Remote {
            code: "rate_limited".into(),
            message: "retry in 30 s".into(),
        }));
        assert_eq!(reserve(&api, 1, Some(&entitlements(None))).unwrap_err().code(), "rate_limited");
    }

    #[test]
    fn reservation_updates_the_cached_account() {
        let tmp = tempfile::tempdir().unwrap();
        let (engine, events) = signed_in_engine(tmp.path(), Some(200));
        let api = FakeUsage::reserving(Ok(Reserve::Granted { unlimited: false, usage: usage(Some(200), 52) }));
        assert!(engine.reserve_with(&api, 40).unwrap(), "limited: unused photos are given back");
        let AuthState::SignedIn { account } = engine.auth_status() else { panic!("signed out") };
        assert_eq!(account.entitlements.used_this_month, 52);
        let (name, payload) = events.0.lock().last().cloned().unwrap();
        assert_eq!((name.as_str(), &payload["account"]["entitlements"]["usedThisMonth"]), ("auth-changed", &52.into()));

        // Refused: the cached count follows Pelli's too.
        let api = FakeUsage::reserving(Ok(Reserve::Refused(usage(Some(200), 195))));
        assert_eq!(engine.reserve_with(&api, 40).unwrap_err().code(), "quotaExceeded");
        let AuthState::SignedIn { account } = engine.auth_status() else { panic!("signed out") };
        assert_eq!(account.entitlements.used_this_month, 195);

        // Unlimited: nothing to give back.
        let api = FakeUsage::reserving(Ok(Reserve::Granted { unlimited: true, usage: usage(None, 300) }));
        assert!(!engine.reserve_with(&api, 40).unwrap());
    }

    #[test]
    fn refused_token_signs_out() {
        let tmp = tempfile::tempdir().unwrap();
        let (engine, _) = signed_in_engine(tmp.path(), Some(200));
        let api =
            FakeUsage::reserving(Err(EngineError::Remote { code: "invalid_token".into(), message: "expired".into() }));
        assert_eq!(engine.reserve_with(&api, 3).unwrap_err().code(), "unauthorized");
        assert_eq!(engine.auth_status(), AuthState::SignedOut);
        // Signed out: an analysis with new photos cannot be counted.
        assert_eq!(engine.reserve_usage(3).unwrap_err().code(), "unauthorized");
        // Nothing new to count: no account needed.
        assert!(!engine.reserve_usage(0).unwrap());

        // A blocked account signs out the same way.
        let tmp = tempfile::tempdir().unwrap();
        let (engine, _) = signed_in_engine(tmp.path(), None);
        let api = FakeUsage::reserving(Err(EngineError::Remote {
            code: "account_unavailable".into(),
            message: "banned".into(),
        }));
        assert_eq!(engine.reserve_with(&api, 3).unwrap_err().code(), "unauthorized");
        assert_eq!(engine.auth_status(), AuthState::SignedOut);
    }

    #[test]
    fn release_is_best_effort() {
        let tmp = tempfile::tempdir().unwrap();
        let (engine, _) = signed_in_engine(tmp.path(), Some(200));
        let api = FakeUsage::releasing(Ok(7));
        engine.release_with(&api, 5);
        assert_eq!(*api.calls.lock(), ["release 5"]);
        let AuthState::SignedIn { account } = engine.auth_status() else { panic!("signed out") };
        assert_eq!(account.entitlements.used_this_month, 7);
        // A failure leaves the cached count alone.
        engine.release_with(&FakeUsage::releasing(Err(offline())), 5);
        let AuthState::SignedIn { account } = engine.auth_status() else { panic!("signed out") };
        assert_eq!(account.entitlements.used_this_month, 7);
    }

    #[test]
    fn next_month() {
        let ms = |y: i32, m: u32, d: u32, h: u32| -> i64 {
            // Days since the epoch by counting, independent of the code above.
            let leap = |y: i32| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
            let month_days = |y: i32, m: u32| match m {
                2 if leap(y) => 29,
                2 => 28,
                4 | 6 | 9 | 11 => 30,
                _ => 31,
            };
            let mut days: i64 = (1970..y).map(|y| if leap(y) { 366 } else { 365 }).sum();
            days += (1..m).map(|m| month_days(y, m)).sum::<i64>() + i64::from(d - 1);
            days * 86_400_000 + i64::from(h) * 3_600_000
        };
        assert_eq!(next_month_utc(ms(2026, 11, 4, 15)), ms(2026, 12, 1, 0));
        assert_eq!(next_month_utc(ms(2026, 12, 31, 23)), ms(2027, 1, 1, 0));
        assert_eq!(next_month_utc(ms(2028, 2, 29, 12)), ms(2028, 3, 1, 0));
        assert_eq!(next_month_utc(ms(2026, 12, 1, 0)), ms(2027, 1, 1, 0));
        assert_eq!(next_month_utc(0), ms(1970, 2, 1, 0));
    }
}
