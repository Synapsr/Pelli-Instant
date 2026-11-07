//! Client of the Pelli API (`/api/cull/*`, see `docs/pelli-api.md`).

use crate::{EngineError, Result};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use std::time::Duration;

/// Pelli's production origin.
pub const DEFAULT_API_URL: &str = "https://app.pelli.io";

/// Origin of the Pelli API: `PELLI_API_URL`, else [`DEFAULT_API_URL`].
///
/// Release builds take `PELLI_API_URL` from the build environment and never
/// read it at run time: an official build cannot be pointed at another
/// server (a fake Pelli would lift the monthly quota); doing so means
/// building from source. Debug builds (`pnpm tauri dev`, `pnpm
/// app:fake-pelli`, the dev bridge) read it at run time.
pub fn api_url() -> String {
    origin(configured_api_url().as_deref())
}

#[cfg(debug_assertions)]
fn configured_api_url() -> Option<String> {
    std::env::var("PELLI_API_URL").ok()
}

#[cfg(not(debug_assertions))]
fn configured_api_url() -> Option<String> {
    option_env!("PELLI_API_URL").map(str::to_owned)
}

/// `configured` without trailing slash when it is an HTTP(S) URL, else the
/// production origin.
fn origin(configured: Option<&str>) -> String {
    configured
        .map(str::trim)
        .filter(|s| s.starts_with("https://") || s.starts_with("http://"))
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or_else(|| DEFAULT_API_URL.to_string())
}

pub fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

/// HTTP agent: OS trust store (corporate proxies), proxy from the
/// environment, errors returned as responses so we can read their JSON.
pub fn agent(timeout: Duration) -> ureq::Agent {
    use ureq::tls::{RootCerts, TlsConfig};
    ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .user_agent(format!("PelliInstant/{} ({})", env!("CARGO_PKG_VERSION"), platform()))
        .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
        .proxy(ureq::Proxy::try_from_env())
        .build()
        .into()
}

// ---------------------------------------------------------------------------
// Responses and errors
// ---------------------------------------------------------------------------

/// Error envelope of the Pelli API: `{ code, message, error }`, where `error`
/// repeats the message (older routes put the code there instead).
#[derive(Debug, Deserialize)]
struct ErrorBody {
    code: Option<String>,
    message: Option<String>,
    error: Option<String>,
}

impl ErrorBody {
    /// The stable machine code: `code`, else `error` when it is one
    /// (`quota_exceeded`, not a sentence).
    fn code(&self) -> Option<&str> {
        let is_code = |s: &&str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_');
        self.code.as_deref().or_else(|| self.error.as_deref().filter(is_code))
    }
}

fn remote(code: &str, message: impl Into<String>) -> EngineError {
    EngineError::Remote { code: code.to_string(), message: message.into() }
}

fn net(e: ureq::Error) -> EngineError {
    remote("network", e.to_string())
}

/// True when Pelli could not be reached or answered something that is not
/// the API (no network, captive portal, proxy or server down).
pub fn unreachable(e: &EngineError) -> bool {
    matches!(e, EngineError::Remote { code, .. }
        if code == "network" || code == "bad_response" || code.starts_with("http_5"))
}

/// Status and body of a response (HTTP errors are responses, see [`agent`]).
fn body_text(mut resp: ureq::http::Response<ureq::Body>) -> Result<(u16, String)> {
    let status = resp.status().as_u16();
    let text = resp
        .body_mut()
        .with_config()
        .limit(32 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| remote("network", e.to_string()))?;
    Ok((status, text))
}

/// Error code of an error response body, if it carries one.
fn error_code(text: &str) -> Option<String> {
    serde_json::from_str::<ErrorBody>(text).ok()?.code().map(str::to_owned)
}

/// Decodes a 2xx body, or turns an error response into
/// [`EngineError::Remote`] with Pelli's error code.
fn decode<T: DeserializeOwned>(status: u16, text: &str) -> Result<T> {
    if (200..300).contains(&status) {
        return serde_json::from_str(text).map_err(|e| remote("bad_response", e.to_string()));
    }
    let body: Option<ErrorBody> = serde_json::from_str(text).ok();
    let code = body.as_ref().and_then(ErrorBody::code).map(str::to_owned).unwrap_or_else(|| match status {
        401 => "unauthorized".into(),
        403 => "forbidden".into(),
        404 => "not_found".into(),
        429 => "rate_limited".into(),
        _ => format!("http_{status}"),
    });
    let message = body.and_then(|b| b.message.or(b.error)).unwrap_or_else(|| format!("HTTP {status}"));
    Err(remote(&code, message))
}

// ---------------------------------------------------------------------------
// Account
// ---------------------------------------------------------------------------

/// Pelli plan of the account.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Plan {
    #[default]
    Free,
    Lite,
    Pro,
    Ultra,
}

/// Lenient: a plan this version does not know (added later in Pelli) reads
/// as [`Plan::Free`], the most restrictive one.
impl<'de> Deserialize<'de> for Plan {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        Ok(match serde_json::Value::deserialize(d)?.as_str() {
            Some("lite") => Plan::Lite,
            Some("pro") => Plan::Pro,
            Some("ultra") => Plan::Ultra,
            _ => Plan::Free,
        })
    }
}

/// What the account may do.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entitlements {
    /// The account may sort photos (not suspended).
    pub analyze: bool,
    /// Photos sorted per calendar month (UTC); `None` = unlimited.
    pub monthly_quota: Option<u32>,
    pub used_this_month: u32,
    /// First instant of next month (UTC, epoch ms), when the count resets.
    pub resets_at: i64,
    /// `false` behind the Pelli paywall (no plan, no running trial).
    pub send_to_gallery: bool,
}

impl Default for Entitlements {
    fn default() -> Self {
        Entitlements {
            analyze: true,
            monthly_quota: None,
            used_this_month: 0,
            resets_at: crate::usage::next_month_utc(crate::now_ms()),
            send_to_gallery: false,
        }
    }
}

/// The signed-in Pelli account (`me`), passed through to the UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub name: String,
    pub email: String,
    pub slug: String,
    pub avatar_url: Option<String>,
    pub plan: Plan,
    /// Localised by Pelli: "Gratuit", "Pro", "Essai Ultra", "Ultra à vie"…
    pub plan_label: String,
    /// When the running Pelli trial ends (epoch ms), `None` without trial.
    pub trial_ends_at: Option<i64>,
    /// The account may still start a Pelli trial.
    pub trial_available: bool,
    /// Lifetime deal (no subscription).
    pub lifetime: bool,
    pub entitlements: Entitlements,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedSession {
    pub session_id: String,
    pub auth_url: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCheck {
    pub status: String,
    pub access_token: Option<String>,
    pub account: Option<Account>,
}

// ---------------------------------------------------------------------------
// Monthly usage
// ---------------------------------------------------------------------------

/// Monthly usage as Pelli counts it (`usage/reserve` answers and
/// `quota_exceeded` errors).
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct QuotaUsage {
    /// Photos per calendar month (UTC); `None` = unlimited.
    pub monthly_quota: Option<u32>,
    pub used_this_month: u32,
    /// Photos left this month; `None` = unlimited.
    pub remaining: Option<u32>,
    /// First instant of next month (UTC, epoch ms).
    pub resets_at: i64,
}

/// Answer of `usage/reserve`.
#[derive(Debug, Clone, PartialEq)]
pub enum Reserve {
    /// The photos are counted; `unlimited` when the plan has no quota.
    Granted { unlimited: bool, usage: QuotaUsage },
    /// The quota left is too small: nothing was counted.
    Refused(QuotaUsage),
}

// ---------------------------------------------------------------------------
// Offers
// ---------------------------------------------------------------------------

/// Pelli's current offers (`offers`), passed through to the UI so prices
/// and links stay right when Pelli changes them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Offers {
    /// ISO 4217 code of the prices.
    pub currency: String,
    pub trial_days: u32,
    pub plans: Vec<PlanOffer>,
    pub links: OfferLinks,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PlanOffer {
    /// `free`, `lite`, `pro`, `ultra` (passed through as Pelli sends it).
    pub slug: String,
    pub name: String,
    /// Price per month, billed monthly.
    pub monthly_price: f64,
    /// Price of a year, billed yearly.
    pub yearly_price: f64,
    /// The same, per month.
    pub yearly_price_per_month: f64,
    /// Photos sorted per month; `None` = unlimited.
    pub photos_per_month: Option<u32>,
    /// What the plan brings in Pelli itself, localised.
    pub highlights: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct OfferLinks {
    pub register: String,
    pub pricing: String,
    pub trial: String,
    pub billing: String,
}

// ---------------------------------------------------------------------------
// Galleries
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GallerySummary {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub status: String,
    pub image_count: u32,
    pub cover_url: Option<String>,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRef {
    pub id: String,
    pub name: String,
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

pub struct PelliClient {
    base: String,
    token: Option<String>,
    /// App language, sent as `Accept-Language` (localises `planLabel` and
    /// error messages).
    language: String,
    agent: ureq::Agent,
}

impl PelliClient {
    pub fn new(token: Option<String>, language: &str) -> PelliClient {
        PelliClient { base: api_url(), token, language: language.to_string(), agent: agent(Duration::from_secs(30)) }
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    /// `POST /api/cull/{path}` with a JSON body: status and response body.
    fn send<B: Serialize>(&self, path: &str, body: &B) -> Result<(u16, String)> {
        let mut req = self.agent.post(format!("{}/api/cull/{path}", self.base));
        if let Some(t) = &self.token {
            req = req.header("Authorization", format!("Bearer {t}"));
        }
        let resp = req.header("Accept-Language", &self.language).send_json(body).map_err(net)?;
        body_text(resp)
    }

    fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let (status, text) = self.send(path, body)?;
        decode(status, &text)
    }

    pub fn create_session(&self, device_name: &str) -> Result<CreatedSession> {
        self.post(
            "auth/create-session",
            &serde_json::json!({
                "deviceName": device_name,
                "platform": platform(),
                "appVersion": env!("CARGO_PKG_VERSION"),
            }),
        )
    }

    pub fn check_session(&self, session_id: &str) -> Result<SessionCheck> {
        self.post("auth/check-session", &serde_json::json!({ "sessionId": session_id }))
    }

    pub fn me(&self) -> Result<Account> {
        #[derive(Deserialize)]
        struct R {
            account: Account,
        }
        Ok(self.post::<_, R>("me", &serde_json::json!({}))?.account)
    }

    /// `usage/reserve`: counts `photos` on the monthly quota, all or nothing.
    pub fn reserve_usage(&self, photos: u32) -> Result<Reserve> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Reservation {
            granted: bool,
            #[serde(default)]
            unlimited: bool,
            #[serde(flatten)]
            usage: QuotaUsage,
        }
        let (status, text) = self.send("usage/reserve", &serde_json::json!({ "photos": photos }))?;
        if status == 403 && error_code(&text).as_deref() == Some("quota_exceeded") {
            let usage = serde_json::from_str(&text).map_err(|e| remote("bad_response", e.to_string()))?;
            return Ok(Reserve::Refused(usage));
        }
        let r: Reservation = decode(status, &text)?;
        Ok(if r.granted {
            Reserve::Granted { unlimited: r.unlimited, usage: r.usage }
        } else {
            Reserve::Refused(r.usage)
        })
    }

    /// `usage/release`: gives back photos reserved but not analysed. Returns
    /// the new `usedThisMonth`.
    pub fn release_usage(&self, photos: u32) -> Result<u32> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct R {
            used_this_month: u32,
        }
        Ok(self.post::<_, R>("usage/release", &serde_json::json!({ "photos": photos }))?.used_this_month)
    }

    /// `GET offers?lang=` (no token needed).
    pub fn offers(&self, lang: &str) -> Result<Offers> {
        let url = format!("{}/api/cull/offers?lang={}", self.base, utf8_percent_encode(lang, NON_ALPHANUMERIC));
        let resp = self.agent.get(url).header("Accept-Language", lang).call().map_err(net)?;
        let (status, text) = body_text(resp)?;
        decode(status, &text)
    }

    pub fn list_galleries(&self) -> Result<Vec<GallerySummary>> {
        #[derive(Deserialize)]
        struct R {
            galleries: Vec<GallerySummary>,
        }
        Ok(self.post::<_, R>("galleries/list", &serde_json::json!({}))?.galleries)
    }

    pub fn create_gallery(&self, request_id: &str, title: &str, kind: &str) -> Result<GallerySummary> {
        #[derive(Deserialize)]
        struct R {
            gallery: GallerySummary,
        }
        Ok(self
            .post::<_, R>(
                "galleries/create",
                &serde_json::json!({ "requestId": request_id, "title": title, "type": kind }),
            )?
            .gallery)
    }

    pub fn create_folder(&self, request_id: &str, gallery_id: &str, name: &str) -> Result<FolderRef> {
        #[derive(Deserialize)]
        struct R {
            folder: FolderRef,
        }
        Ok(self
            .post::<_, R>(
                "folders/create",
                &serde_json::json!({ "requestId": request_id, "galleryId": gallery_id, "name": name }),
            )?
            .folder)
    }

    pub fn upload_token(&self, gallery_id: &str) -> Result<serde_json::Value> {
        self.post("upload-token", &serde_json::json!({ "galleryId": gallery_id }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn api_origin() {
        assert_eq!(origin(None), DEFAULT_API_URL);
        assert_eq!(origin(Some("http://localhost:3100/")), "http://localhost:3100");
        assert_eq!(origin(Some(" https://staging.pelli.io ")), "https://staging.pelli.io");
        assert_eq!(origin(Some("localhost:3100")), DEFAULT_API_URL);
        assert_eq!(origin(Some("")), DEFAULT_API_URL);
    }

    #[test]
    fn account_reads_from_pelli() {
        let a: Account = serde_json::from_value(json!({
            "name": "Camille", "email": "c@example.com", "slug": "camille", "avatarUrl": null,
            "plan": "lite", "planLabel": "Lite", "trialEndsAt": null, "trialAvailable": false, "lifetime": false,
            "entitlements": { "analyze": true, "monthlyQuota": 1000, "usedThisMonth": 12,
                              "resetsAt": 1_735_689_600_000_i64, "sendToGallery": true }
        }))
        .unwrap();
        assert_eq!(
            (a.plan, a.entitlements.monthly_quota, a.entitlements.used_this_month),
            (Plan::Lite, Some(1000), 12)
        );
    }

    #[test]
    fn account_json_is_camel_case() {
        let v = serde_json::to_value(Account { plan: Plan::Ultra, ..Default::default() }).unwrap();
        assert_eq!(v["plan"], "ultra");
        assert!(v["entitlements"]["monthlyQuota"].is_null());
        assert!(v.get("trialEndsAt").is_some() && v["entitlements"].get("usedThisMonth").is_some());
    }

    #[test]
    fn error_codes() {
        let err = |status, text: &str| decode::<serde_json::Value>(status, text).unwrap_err().to_string();
        assert_eq!(err(403, r#"{"code":"quota_exceeded","message":"m","error":"m"}"#), "quota_exceeded: m");
        assert_eq!(err(403, r#"{"error":"quota_exceeded","message":"m"}"#), "quota_exceeded: m");
        assert_eq!(err(404, r#"{"error":"Gallery not found."}"#), "not_found: Gallery not found.");
        assert_eq!(err(502, "<html>Bad gateway</html>"), "http_502: HTTP 502");
        assert!(unreachable(&decode::<serde_json::Value>(503, "").unwrap_err()));
        assert!(!unreachable(&decode::<serde_json::Value>(401, "").unwrap_err()));
    }
}
