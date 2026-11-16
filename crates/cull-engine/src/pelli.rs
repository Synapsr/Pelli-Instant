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
/// server (a fake Pelli would hand out Plus); doing so means building from
/// source. Debug builds (`pnpm tauri dev`, `pnpm app:fake-pelli`, the dev
/// bridge) read it at run time.
pub fn api_url() -> String {
    origin(configured_api_url().as_deref())
}

/// `PELLI_API_URL` names a Pelli (else the production one is used).
pub fn api_url_configured() -> bool {
    configured_api_url().is_some_and(|u| !u.trim().is_empty())
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

/// The platform as the Plus and telemetry routes name it: `mac`, `windows`,
/// `linux`.
pub fn os() -> &'static str {
    if cfg!(target_os = "macos") {
        "mac"
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
    /// (`subscription_required`, not a sentence).
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

/// Pelli unreachable reads as `offline` in the UI, whatever the cause.
pub fn offline_if_unreachable(e: EngineError) -> EngineError {
    match e {
        EngineError::Remote { message, .. } if unreachable(&e) => {
            EngineError::Remote { code: "network".into(), message }
        }
        other => other,
    }
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

/// What the account may do, as Pelli sends it (sorting is free and
/// unlimited: the app no longer reads the quota fields, Pelli keeps them for
/// 0.1 apps).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entitlements {
    pub analyze: bool,
    pub monthly_quota: Option<u32>,
    pub used_this_month: u32,
    pub resets_at: i64,
    /// `false` behind the Pelli paywall (no plan, no running trial).
    pub send_to_gallery: bool,
}

impl Default for Entitlements {
    fn default() -> Self {
        Entitlements { analyze: true, monthly_quota: None, used_this_month: 0, resets_at: 0, send_to_gallery: false }
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
    /// The plan includes Pelli Instant Plus: Pro or Ultra, lifetime deals
    /// and running Pro/Ultra trials included. Absent from an account cached
    /// by 0.1.
    #[serde(default)]
    pub plus: bool,
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
// Pelli Instant Plus
// ---------------------------------------------------------------------------

/// Price of Plus where the computer is (`plus/offer`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlusOffer {
    /// Minor units (2500).
    pub amount: u32,
    /// `eur` or `usd`.
    pub currency: String,
    /// Formatted by Pelli: "25 €", "$25".
    pub label: String,
}

/// A Stripe Checkout waiting for payment (`plus/checkout`).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Checkout {
    pub id: String,
    pub claim_token: String,
    pub url: String,
}

/// `plus/claim`: `pending`, `active` (with the licence) or `expired`.
#[derive(Debug, Clone, Deserialize)]
pub struct Claim {
    pub status: String,
    pub key: Option<String>,
    pub license: Option<String>,
}

/// A licence key and its token for this device (`plus/activate`).
#[derive(Debug, Clone, Deserialize)]
pub struct IssuedLicense {
    pub key: String,
    pub license: String,
}

/// A file of the Maximum pack and where to download it for a while
/// (`models`). Its size and checksum come from the manifest compiled in, not
/// from here.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelUrl {
    pub file: String,
    pub url: String,
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
    /// Pelli's origin ([`api_url`]).
    base: String,
    token: Option<String>,
    /// App language, sent as `Accept-Language` (localises `planLabel` and
    /// error messages).
    language: String,
    agent: ureq::Agent,
}

impl PelliClient {
    pub fn new(base: &str, token: Option<String>, language: &str) -> PelliClient {
        PelliClient {
            base: base.to_string(),
            token,
            language: language.to_string(),
            agent: agent(Duration::from_secs(30)),
        }
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

    /// `GET /api/cull/{path}?lang=` (no token).
    fn get<T: DeserializeOwned>(&self, path: &str, lang: &str) -> Result<T> {
        let url = format!("{}/api/cull/{path}?lang={}", self.base, utf8_percent_encode(lang, NON_ALPHANUMERIC));
        let resp = self.agent.get(url).header("Accept-Language", lang).call().map_err(net)?;
        let (status, text) = body_text(resp)?;
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

    pub fn plus_offer(&self, lang: &str) -> Result<PlusOffer> {
        self.get("plus/offer", lang)
    }

    /// Starts a Stripe Checkout for Plus on `device` (its hash).
    pub fn plus_checkout(&self, device: &str, lang: &str) -> Result<Checkout> {
        self.post(
            "plus/checkout",
            &serde_json::json!({
                "device": device,
                "lang": lang,
                "platform": os(),
                "appVersion": env!("CARGO_PKG_VERSION"),
            }),
        )
    }

    pub fn plus_claim(&self, id: &str, claim_token: &str) -> Result<Claim> {
        self.post("plus/claim", &serde_json::json!({ "id": id, "claimToken": claim_token }))
    }

    /// Errors `invalidKey`, `otherDevice` (bound to another device) and
    /// `revoked` (refunded).
    pub fn plus_activate(&self, key: &str, device: &str) -> Result<IssuedLicense> {
        self.post("plus/activate", &serde_json::json!({ "key": key, "device": device }))
    }

    /// Unbinds the licence from its device.
    pub fn plus_release(&self, license: &str) -> Result<()> {
        self.post::<_, serde_json::Value>("plus/release", &serde_json::json!({ "license": license }))?;
        Ok(())
    }

    /// `active`, `released`, `revoked` or `invalid`.
    pub fn plus_verify(&self, license: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct R {
            status: String,
        }
        Ok(self.post::<_, R>("plus/verify", &serde_json::json!({ "license": license }))?.status)
    }

    /// Download URLs of the Maximum pack, for a licence token, or for the
    /// Bearer of a Pelli account whose plan includes Plus (`license: None`).
    /// 403 `plusRequired` otherwise.
    pub fn models(&self, license: Option<&str>) -> Result<Vec<ModelUrl>> {
        #[derive(Deserialize)]
        struct R {
            files: Vec<ModelUrl>,
        }
        let body = match license {
            Some(l) => serde_json::json!({ "license": l }),
            None => serde_json::json!({}),
        };
        Ok(self.post::<_, R>("models", &body)?.files)
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
        assert!(!a.plus, "an account cached by 0.1 has no `plus`");
        let b: Account = serde_json::from_value(json!({
            "name": "Camille", "email": "c@example.com", "slug": "camille", "avatarUrl": null,
            "plan": "pro", "planLabel": "Pro", "trialEndsAt": null, "trialAvailable": false, "lifetime": false,
            "entitlements": { "analyze": true, "monthlyQuota": null, "usedThisMonth": 0, "resetsAt": 0,
                              "sendToGallery": true },
            "plus": true
        }))
        .unwrap();
        assert!(b.plus);
    }

    #[test]
    fn account_json_is_camel_case() {
        let v = serde_json::to_value(Account { plan: Plan::Ultra, ..Default::default() }).unwrap();
        assert_eq!(v["plan"], "ultra");
        assert!(v["entitlements"]["monthlyQuota"].is_null());
        assert!(v.get("trialEndsAt").is_some() && v["entitlements"].get("usedThisMonth").is_some());
        assert_eq!(v["plus"], false);
    }

    #[test]
    fn error_codes() {
        let err = |status, text: &str| decode::<serde_json::Value>(status, text).unwrap_err().to_string();
        assert_eq!(err(403, r#"{"code":"plusRequired","message":"m"}"#), "plusRequired: m");
        assert_eq!(err(409, r#"{"code":"otherDevice","message":"m","error":"m"}"#), "otherDevice: m");
        assert_eq!(err(403, r#"{"error":"subscription_required","message":"m"}"#), "subscription_required: m");
        assert_eq!(err(404, r#"{"error":"Gallery not found."}"#), "not_found: Gallery not found.");
        assert_eq!(err(502, "<html>Bad gateway</html>"), "http_502: HTTP 502");
        assert!(unreachable(&decode::<serde_json::Value>(503, "").unwrap_err()));
        assert!(!unreachable(&decode::<serde_json::Value>(401, "").unwrap_err()));
        let down = offline_if_unreachable(decode::<serde_json::Value>(502, "").unwrap_err());
        assert_eq!(down.code(), "offline");
        let refused = offline_if_unreachable(decode::<serde_json::Value>(410, r#"{"code":"revoked"}"#).unwrap_err());
        assert_eq!(refused.code(), "revoked");
    }
}
