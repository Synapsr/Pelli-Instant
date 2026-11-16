//! Pelli Instant Plus: the Maximum pack, AI search and no Pelli promotion.
//!
//! Plus comes from a licence bought for this device, or from a signed-in
//! Pelli account whose plan includes it (Pro, Ultra); the licence wins when
//! both. Sorting itself is free and never needs either.
//!
//! # The licence
//!
//! One key (`XXXX-XXXX-XXXX-XXXX`, Crockford base32, [`normalize_key`]) = one
//! device at a time. Pelli binds it to a hash of this device
//! ([`device_hash`]) and hands out a token the app verifies offline
//! ([`verify_token`]):
//!
//! ```text
//! base64url({"v":1,"lid":"…","dev":"<device hash>","iat":…}) "." base64url(Ed25519 signature of the first part)
//! ```
//!
//! signed by Pelli, whose public key is compiled in ([`PUBLIC_KEY`]; debug
//! builds take another one from `PELLI_LICENSE_PUBLIC_KEY`, for a fake
//! Pelli). Offline, the token keeps working; at most once a day when online
//! ([`Engine::start_services`]) the app asks Pelli whether the licence is
//! still active (`plus/verify`) and forgets it otherwise (released on another
//! computer, refunded). Buying goes through a Stripe Checkout in the browser
//! (`plus_checkout`), which the app polls until it is paid (`plus_poll`).
//! Pelli only ever receives the key, the token and the device hash.
//!
//! The licence, the checkout waiting for payment and the date of the last
//! check live in the library's key/value table. Every change of
//! [`PlusStatus`] is sent to the UI as a `plus` event.

use crate::pelli::{self, PlusOffer};
use crate::{now_ms, Engine, EngineError, Result};
use base64::Engine as _;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::OnceLock;

/// Pelli's licence signing key: Ed25519, raw 32 bytes, base64.
pub const PUBLIC_KEY: &str = "Dx2SeBSPVz9jHkz+D/R5ise2ZI2Ir/JcXbKGRfH+7Oo=";

/// kv keys.
const LICENSE_KEY: &str = "plus.license";
const CHECKOUT_KEY: &str = "plus.checkout";
const VERIFIED_KEY: &str = "plus.verified_at";
/// Random id of this device when the system has none (data dir).
const DEVICE_ID_FILE: &str = "device-id";
const VERIFY_EVERY_MS: i64 = 24 * 60 * 60 * 1000;
/// Crockford base32: no I, L, O, U.
const CROCKFORD: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlusSource {
    License,
    Account,
}

/// Plus as the UI sees it (contract `PlusStatus`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlusStatus {
    pub active: bool,
    /// The licence when both.
    pub source: Option<PlusSource>,
    /// The licence key (the UI masks it).
    pub key: Option<String>,
    /// A checkout is waiting for payment.
    pub pending: bool,
}

/// How Plus was activated (`plus_activated` telemetry).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Via {
    Purchase,
    Key,
    Account,
}

// ---------------------------------------------------------------------------
// Licence token, key, device
// ---------------------------------------------------------------------------

/// What a licence token says.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LicenseClaims {
    pub v: u32,
    /// Licence id.
    pub lid: String,
    /// Device hash.
    pub dev: String,
    /// Issued at (unix seconds).
    pub iat: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenError {
    Malformed,
    Signature,
    Version,
    /// Issued for another device.
    Device,
}

/// Checks a licence token offline: Ed25519 signature by `public_key` over
/// the ASCII bytes of its first part, version 1, issued for `device`.
pub fn verify_token(token: &str, public_key: &[u8], device: &str) -> std::result::Result<LicenseClaims, TokenError> {
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let (payload, signature) = token.trim().split_once('.').ok_or(TokenError::Malformed)?;
    let signature = b64.decode(signature).map_err(|_| TokenError::Malformed)?;
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public_key)
        .verify(payload.as_bytes(), &signature)
        .map_err(|_| TokenError::Signature)?;
    let claims: LicenseClaims =
        b64.decode(payload).ok().and_then(|json| serde_json::from_slice(&json).ok()).ok_or(TokenError::Malformed)?;
    if claims.v != 1 {
        return Err(TokenError::Version);
    }
    if claims.dev != device {
        return Err(TokenError::Device);
    }
    Ok(claims)
}

/// A licence key as typed (any case, spaces and dashes anywhere, the
/// Crockford look-alikes I, L → 1 and O → 0) in its canonical form
/// `XXXX-XXXX-XXXX-XXXX`; `None` when it cannot be one.
pub fn normalize_key(input: &str) -> Option<String> {
    let mut chars = Vec::with_capacity(16);
    for c in input.chars() {
        if c.is_whitespace() || matches!(c, '-' | '‐' | '–' | '—') {
            continue;
        }
        let c = match c.to_ascii_uppercase() {
            'I' | 'L' => '1',
            'O' => '0',
            c => c,
        };
        if !c.is_ascii() || !CROCKFORD.contains(&(c as u8)) {
            return None;
        }
        chars.push(c);
    }
    (chars.len() == 16).then(|| chars.chunks(4).map(|g| g.iter().collect::<String>()).collect::<Vec<_>>().join("-"))
}

/// Lowercase hex SHA-256.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// A random UUID v4.
pub(crate) fn random_uuid() -> String {
    use ring::rand::SecureRandom;
    let mut b = [0u8; 16];
    if ring::rand::SystemRandom::new().fill(&mut b).is_err() {
        // No system randomness (never seen): the clock and the process.
        let seed = format!("{:?}{}", std::time::SystemTime::now(), std::process::id());
        b.copy_from_slice(&ring::digest::digest(&ring::digest::SHA256, seed.as_bytes()).as_ref()[..16]);
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

/// This device's hash: lowercase hex SHA-256 of `pelli-instant-device-v1:`
/// + the machine id (macOS IOPlatformUUID, Windows MachineGuid, Linux
///   `/etc/machine-id`), or of a random id created once in `data_dir` when the
///   system has none.
pub fn device_hash(data_dir: &Path) -> String {
    let id = machine_id().unwrap_or_else(|| fallback_device_id(data_dir));
    device_hash_of(&id)
}

fn device_hash_of(machine_id: &str) -> String {
    sha256_hex(format!("pelli-instant-device-v1:{machine_id}").as_bytes())
}

fn fallback_device_id(data_dir: &Path) -> String {
    let path = data_dir.join(DEVICE_ID_FILE);
    if let Some(id) = std::fs::read_to_string(&path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        return id;
    }
    let id = random_uuid();
    if let Err(e) = std::fs::create_dir_all(data_dir).and_then(|()| std::fs::write(&path, &id)) {
        log::warn!("device id not saved in {}: {e}", path.display());
    }
    id
}

/// IOPlatformUUID, read with `ioreg` (part of every macOS: no IOKit binding
/// to maintain).
#[cfg(target_os = "macos")]
fn machine_id() -> Option<String> {
    let out =
        std::process::Command::new("/usr/sbin/ioreg").args(["-rd1", "-c", "IOPlatformExpertDevice"]).output().ok()?;
    parse_ioreg_uuid(&String::from_utf8_lossy(&out.stdout))
}

/// `"IOPlatformUUID" = "…"` in `ioreg`'s output.
#[cfg(any(target_os = "macos", test))]
fn parse_ioreg_uuid(text: &str) -> Option<String> {
    let line = text.lines().find(|l| l.contains("\"IOPlatformUUID\""))?;
    let value = line.split_once('=')?.1.trim().trim_matches('"').trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// `HKLM\SOFTWARE\Microsoft\Cryptography\MachineGuid` (64-bit view).
#[cfg(target_os = "windows")]
fn machine_id() -> Option<String> {
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY,
    };
    let key: Vec<u16> = "SOFTWARE\\Microsoft\\Cryptography\0".encode_utf16().collect();
    let value: Vec<u16> = "MachineGuid\0".encode_utf16().collect();
    let mut buf = [0u16; 128];
    let mut bytes = std::mem::size_of_val(&buf) as u32;
    // SAFETY: NUL-terminated key and value names; `bytes` is the size of
    // `buf` in bytes, and RegGetValueW writes at most that much.
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if rc != 0 {
        return None;
    }
    let len = (bytes as usize / 2).min(buf.len());
    let id = String::from_utf16_lossy(&buf[..len]);
    let id = id.trim_end_matches('\0').trim();
    (!id.is_empty()).then(|| id.to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn machine_id() -> Option<String> {
    ["/etc/machine-id", "/var/lib/dbus/machine-id"]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
}

/// The public key licence tokens are checked against (see the module docs).
fn public_key() -> Vec<u8> {
    let decode =
        |k: &str| base64::engine::general_purpose::STANDARD.decode(k.trim()).ok().filter(|k: &Vec<u8>| k.len() == 32);
    #[cfg(debug_assertions)]
    if let Some(k) = std::env::var("PELLI_LICENSE_PUBLIC_KEY").ok().as_deref().and_then(decode) {
        return k;
    }
    decode(PUBLIC_KEY).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// What the engine keeps for Plus besides the key/value table.
pub(crate) struct PlusState {
    pub(crate) public_key: RwLock<Vec<u8>>,
    /// [`device_hash`], computed once.
    pub(crate) device: OnceLock<String>,
    /// One change of the licence or the checkout at a time (the UI polls
    /// on a timer and on focus): each change is announced once.
    changing: Mutex<()>,
}

impl PlusState {
    pub(crate) fn new() -> PlusState {
        PlusState { public_key: RwLock::new(public_key()), device: OnceLock::new(), changing: Mutex::new(()) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredLicense {
    pub(crate) key: String,
    pub(crate) token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PendingCheckout {
    id: String,
    claim_token: String,
}

fn remote(code: &str, message: impl Into<String>) -> EngineError {
    EngineError::Remote { code: code.into(), message: message.into() }
}

impl Engine {
    /// This device's hash ([`device_hash`]).
    pub fn device_hash(&self) -> String {
        self.plus.device.get_or_init(|| device_hash(&self.config.data_dir)).clone()
    }

    fn kv_json<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.kv_get(key).ok().flatten().and_then(|j| serde_json::from_str(&j).ok())
    }

    /// The stored licence, when its token is valid on this device.
    pub(crate) fn valid_license(&self) -> Option<StoredLicense> {
        let license: StoredLicense = self.kv_json(LICENSE_KEY)?;
        match verify_token(&license.token, &self.plus.public_key.read(), &self.device_hash()) {
            Ok(_) => Some(license),
            Err(e) => {
                log::warn!("the stored Plus licence is not valid on this device: {e:?}");
                None
            }
        }
    }

    /// Signed in with a Pelli account whose plan includes Plus.
    pub(crate) fn account_plus(&self) -> bool {
        self.cached_account().is_some_and(|a| a.plus) && self.token().is_some()
    }

    pub fn plus_status(&self) -> PlusStatus {
        let pending = self.kv_get(CHECKOUT_KEY).ok().flatten().is_some();
        if let Some(license) = self.valid_license() {
            return PlusStatus { active: true, source: Some(PlusSource::License), key: Some(license.key), pending };
        }
        if self.account_plus() {
            return PlusStatus { active: true, source: Some(PlusSource::Account), key: None, pending };
        }
        PlusStatus { pending, ..Default::default() }
    }

    pub fn plus_active(&self) -> bool {
        self.valid_license().is_some() || self.account_plus()
    }

    /// Tells the UI when Plus changed since `before` (`plus` event), and
    /// Pelli when it was just activated (`plus_activated`, `via`).
    pub(crate) fn plus_changed(&self, before: &PlusStatus, via: Via) {
        let after = self.plus_status();
        if &after == before {
            return;
        }
        if after.active && !before.active {
            self.track("plus_activated", serde_json::json!({ "via": via }));
        }
        if let Ok(v) = serde_json::to_value(&after) {
            self.events().emit("plus", v);
        }
    }

    fn store_license(&self, key: &str, token: &str) -> Result<()> {
        verify_token(token, &self.plus.public_key.read(), &self.device_hash()).map_err(|e| {
            remote("internal", format!("the licence Pelli returned does not verify on this device ({e:?})"))
        })?;
        let license = StoredLicense { key: normalize_key(key).unwrap_or_else(|| key.to_string()), token: token.into() };
        self.kv_set(LICENSE_KEY, &serde_json::to_string(&license)?)?;
        // Fresh from Pelli: no need to check it today.
        self.kv_set(VERIFIED_KEY, &now_ms().to_string())
    }

    fn forget_license(&self) -> Result<()> {
        self.kv_delete(LICENSE_KEY)?;
        self.kv_delete(VERIFIED_KEY)
    }

    /// `lang` when it is an app language, else the current one.
    fn plus_lang(&self, lang: &str) -> String {
        if crate::settings::LANGUAGES.contains(&lang) {
            lang.to_string()
        } else {
            self.language()
        }
    }

    /// The price of Plus where the computer is; `None` offline.
    pub fn plus_offer(&self, lang: &str) -> Result<Option<PlusOffer>> {
        match self.pelli(None).plus_offer(&self.plus_lang(lang)) {
            Ok(offer) => Ok(Some(offer)),
            Err(e) if pelli::unreachable(&e) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Starts a checkout for this device and remembers it until it is paid,
    /// expired or cancelled. Returns the Stripe Checkout URL (the host opens
    /// it).
    pub fn plus_checkout(&self, lang: &str) -> Result<String> {
        let _changing = self.plus.changing.lock();
        let before = self.plus_status();
        let checkout = self
            .pelli(None)
            .plus_checkout(&self.device_hash(), &self.plus_lang(lang))
            .map_err(pelli::offline_if_unreachable)?;
        let pending = PendingCheckout { id: checkout.id, claim_token: checkout.claim_token };
        self.kv_set(CHECKOUT_KEY, &serde_json::to_string(&pending)?)?;
        self.plus_changed(&before, Via::Purchase);
        Ok(checkout.url)
    }

    /// Asks Pelli once whether the pending checkout is paid. Offline or
    /// unanswered: still pending.
    pub fn plus_poll(&self) -> Result<PlusStatus> {
        let _changing = self.plus.changing.lock();
        let before = self.plus_status();
        let Some(pending) = self.kv_json::<PendingCheckout>(CHECKOUT_KEY) else { return Ok(before) };
        let claim = match self.pelli(None).plus_claim(&pending.id, &pending.claim_token) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("Plus checkout not checked: {e}");
                return Ok(before);
            }
        };
        match (claim.status.as_str(), claim.key, claim.license) {
            ("pending", ..) => return Ok(before),
            ("active", Some(key), Some(token)) => self.store_license(&key, &token)?,
            ("active", ..) => return Err(remote("bad_response", "paid checkout without licence")),
            // `expired`: the checkout session ended unpaid.
            _ => {}
        }
        self.kv_delete(CHECKOUT_KEY)?;
        self.plus_changed(&before, Via::Purchase);
        Ok(self.plus_status())
    }

    /// Forgets the pending checkout (the photographer closed the dialog).
    pub fn plus_cancel_checkout(&self) -> Result<PlusStatus> {
        let _changing = self.plus.changing.lock();
        let before = self.plus_status();
        self.kv_delete(CHECKOUT_KEY)?;
        self.plus_changed(&before, Via::Purchase);
        Ok(self.plus_status())
    }

    /// Activates a licence key on this device. Errors `invalidKey`,
    /// `otherDevice`, `revoked`, `offline`.
    pub fn plus_activate(&self, key: &str) -> Result<PlusStatus> {
        let key = normalize_key(key).ok_or_else(|| remote("invalidKey", "not a licence key"))?;
        let _changing = self.plus.changing.lock();
        let before = self.plus_status();
        let issued =
            self.pelli(None).plus_activate(&key, &self.device_hash()).map_err(pelli::offline_if_unreachable)?;
        self.store_license(&issued.key, &issued.license)?;
        self.plus_changed(&before, Via::Key);
        Ok(self.plus_status())
    }

    /// Frees the licence so it can be activated on another computer, then
    /// forgets it. Offline: error `offline`, nothing changes.
    pub fn plus_release(&self) -> Result<PlusStatus> {
        let _changing = self.plus.changing.lock();
        let before = self.plus_status();
        let Some(license) = self.kv_json::<StoredLicense>(LICENSE_KEY) else { return Ok(before) };
        self.pelli(None).plus_release(&license.token).map_err(pelli::offline_if_unreachable)?;
        self.forget_license()?;
        self.plus_changed(&before, Via::Key);
        Ok(self.plus_status())
    }

    /// Asks Pelli whether the licence is still active, at most once a day:
    /// anything but `active` forgets it. Offline: nothing changes.
    pub(crate) fn verify_license_if_due(&self) {
        let _changing = self.plus.changing.lock();
        let Some(license) = self.kv_json::<StoredLicense>(LICENSE_KEY) else { return };
        let now = now_ms();
        let last: Option<i64> = self.kv_get(VERIFIED_KEY).ok().flatten().and_then(|t| t.parse().ok());
        if last.is_some_and(|t| t <= now && now - t < VERIFY_EVERY_MS) {
            return;
        }
        let before = self.plus_status();
        match self.pelli(None).plus_verify(&license.token) {
            Ok(status) if status == "active" => {
                if let Err(e) = self.kv_set(VERIFIED_KEY, &now.to_string()) {
                    log::warn!("licence check not recorded: {e}");
                }
            }
            Ok(status) => {
                log::warn!("Plus licence {status}: removed from this computer");
                if let Err(e) = self.forget_license() {
                    log::warn!("licence not removed: {e}");
                }
                self.plus_changed(&before, Via::Key);
            }
            Err(e) => log::info!("Plus licence not checked: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pelli::{Account, Plan};
    use crate::testing::{fake_pelli, sign_token, test_engine, Recorder, TestSender, TEST_SEED};
    use serde_json::json;
    use std::sync::Arc;

    fn test_key() -> Vec<u8> {
        use ring::signature::KeyPair;
        ring::signature::Ed25519KeyPair::from_seed_unchecked(&TEST_SEED).unwrap().public_key().as_ref().to_vec()
    }

    const DEV: &str = "6a1f0c3e9d2b4a5f8e7c6b5a4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e";

    #[test]
    fn tokens_verify_offline() {
        let token = sign_token(&json!({ "v": 1, "lid": "lic_1", "dev": DEV, "iat": 1_763_000_000 }));
        let claims = verify_token(&token, &test_key(), DEV).unwrap();
        assert_eq!((claims.lid.as_str(), claims.iat), ("lic_1", 1_763_000_000));
        // Another device.
        assert_eq!(verify_token(&token, &test_key(), &"0".repeat(64)), Err(TokenError::Device));
        // Another signer: Pelli's real key.
        let pelli = base64::engine::general_purpose::STANDARD.decode(PUBLIC_KEY).unwrap();
        assert_eq!(pelli.len(), 32);
        assert_eq!(verify_token(&token, &pelli, DEV), Err(TokenError::Signature));
        // Tampered payload, signature kept.
        let (_, sig) = token.split_once('.').unwrap();
        let forged = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(json!({ "v": 1, "lid": "lic_2", "dev": DEV, "iat": 1_763_000_000 }).to_string());
        assert_eq!(verify_token(&format!("{forged}.{sig}"), &test_key(), DEV), Err(TokenError::Signature));
        // Unknown version, properly signed.
        let v2 = sign_token(&json!({ "v": 2, "lid": "lic_1", "dev": DEV, "iat": 1 }));
        assert_eq!(verify_token(&v2, &test_key(), DEV), Err(TokenError::Version));
        for bad in ["", "abc", "a.b", "a.b.c", "....", &format!("{}.", token.split('.').next().unwrap())] {
            assert!(verify_token(bad, &test_key(), DEV).is_err(), "{bad}");
        }
        let missing_dev = sign_token(&json!({ "v": 1, "lid": "lic_1", "iat": 1 }));
        assert_eq!(verify_token(&missing_dev, &test_key(), DEV), Err(TokenError::Malformed));
    }

    #[test]
    fn keys_are_normalised() {
        assert_eq!(normalize_key("7k2q-ab3d-9xyz-0mnp").as_deref(), Some("7K2Q-AB3D-9XYZ-0MNP"));
        assert_eq!(normalize_key(" 7K2QAB3D 9XYZ0MNP\n").as_deref(), Some("7K2Q-AB3D-9XYZ-0MNP"));
        assert_eq!(normalize_key("7K2Q–AB3D—9XYZ-0MNP").as_deref(), Some("7K2Q-AB3D-9XYZ-0MNP"));
        // Crockford look-alikes.
        assert_eq!(normalize_key("oIl1-0000-0000-0000").as_deref(), Some("0111-0000-0000-0000"));
        for bad in [
            "",
            "7K2Q-AB3D-9XYZ",
            "7K2Q-AB3D-9XYZ-0MNP-1",
            "7K2Q-AB3D-9XYZ-0MNU",
            "7K2Q-AB3D-9XYZ-0MN!",
            "ÉK2Q-AB3D-9XYZ-0MNP",
        ] {
            assert_eq!(normalize_key(bad), None, "{bad}");
        }
    }

    #[test]
    fn device_hash_format_and_stability() {
        let tmp = tempfile::tempdir().unwrap();
        let a = device_hash(tmp.path());
        assert_eq!(a.len(), 64);
        assert!(a.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)), "{a}");
        assert_eq!(device_hash(tmp.path()), a, "stable");
        assert_eq!(
            device_hash_of("abc"),
            sha256_hex(b"pelli-instant-device-v1:abc"),
            "SHA-256 of the prefixed machine id"
        );
        // Without a machine id: a random id kept in the data dir.
        let id = fallback_device_id(tmp.path());
        assert_eq!(fallback_device_id(tmp.path()), id);
        assert_eq!(id.len(), 36);
        assert_ne!(fallback_device_id(&tmp.path().join("other")), id);
        let ioreg = "+-o J314sAP  <class IOPlatformExpertDevice>\n    \"IOPlatformUUID\" = \"1B2C3D4E-0000-1111-2222-333344445555\"\n";
        assert_eq!(parse_ioreg_uuid(ioreg).as_deref(), Some("1B2C3D4E-0000-1111-2222-333344445555"));
        assert_eq!(parse_ioreg_uuid("nothing"), None);
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let u = random_uuid();
        assert_eq!((u.len(), &u[14..15]), (36, "4"));
        assert_ne!(random_uuid(), u);
    }

    /// An engine checking tokens against the test key.
    fn engine(dir: &Path) -> (Arc<Engine>, Arc<Recorder>, Arc<TestSender>) {
        let events = Arc::new(Recorder::default());
        let engine = test_engine(dir, events.clone());
        *engine.plus.public_key.write() = test_key();
        let sender = Arc::new(TestSender::default());
        engine.set_telemetry_sender(sender.clone());
        (engine, events, sender)
    }

    fn token_for(engine: &Engine) -> String {
        sign_token(&json!({ "v": 1, "lid": "lic_1", "dev": engine.device_hash(), "iat": 1 }))
    }

    fn sign_in(engine: &Engine, plus: bool) {
        let account = Account { plan: if plus { Plan::Pro } else { Plan::Free }, plus, ..Default::default() };
        engine.kv_set("pelli.token", "bearer").unwrap();
        engine.kv_set("pelli.account", &serde_json::to_string(&account).unwrap()).unwrap();
    }

    #[test]
    fn the_licence_wins_over_the_account() {
        let tmp = tempfile::tempdir().unwrap();
        let (engine, ..) = engine(tmp.path());
        assert_eq!(engine.plus_status(), PlusStatus::default());
        sign_in(&engine, false);
        assert!(!engine.plus_active(), "a Free account");
        sign_in(&engine, true);
        assert_eq!(
            engine.plus_status(),
            PlusStatus { active: true, source: Some(PlusSource::Account), key: None, pending: false }
        );
        engine.store_license("7k2q-ab3d-9xyz-0mnp", &token_for(&engine)).unwrap();
        let status = engine.plus_status();
        assert_eq!((status.source, status.key.as_deref()), (Some(PlusSource::License), Some("7K2Q-AB3D-9XYZ-0MNP")));
        assert_eq!(
            serde_json::to_value(&status).unwrap(),
            json!({ "active": true, "source": "license", "key": "7K2Q-AB3D-9XYZ-0MNP", "pending": false })
        );
        // A token of another device is ignored.
        let other = sign_token(&json!({ "v": 1, "lid": "lic_1", "dev": "0".repeat(64), "iat": 1 }));
        engine.kv_set(LICENSE_KEY, &json!({ "key": "K", "token": other }).to_string()).unwrap();
        assert_eq!(engine.plus_status().source, Some(PlusSource::Account));
        assert!(engine.store_license("K", &other).is_err(), "never stored");
        engine.auth_logout().unwrap();
        assert!(!engine.plus_active());
    }

    #[test]
    fn activation_release_and_daily_check() {
        let tmp = tempfile::tempdir().unwrap();
        let (engine, events, sender) = engine(tmp.path());
        let device = engine.device_hash();
        let token = token_for(&engine);
        let verify_status = Arc::new(parking_lot::Mutex::new("active"));
        let (t, status) = (token.clone(), verify_status.clone());
        let pelli = fake_pelli(move |req| match req.path.as_str() {
            "/api/cull/plus/activate" if req.body["key"] == "7K2Q-AB3D-9XYZ-0MNP" => {
                (200, json!({ "key": "7K2Q-AB3D-9XYZ-0MNP", "license": t }))
            }
            "/api/cull/plus/activate" if req.body["key"] == "7K2Q-AB3D-9XYZ-0000" => {
                (409, json!({ "code": "otherDevice", "message": "bound to another device" }))
            }
            "/api/cull/plus/activate" => (404, json!({ "code": "invalidKey", "message": "unknown key" })),
            "/api/cull/plus/release" => (200, json!({ "ok": true })),
            "/api/cull/plus/verify" => (200, json!({ "status": *status.lock() })),
            _ => (404, json!({ "code": "not_found" })),
        });
        *engine.api_url.write() = pelli.url.clone();

        // Malformed keys never reach Pelli.
        assert_eq!(engine.plus_activate("nope").unwrap_err().code(), "invalidKey");
        assert!(pelli.requests().is_empty());
        assert_eq!(engine.plus_activate("7K2Q-AB3D-9XYZ-1111").unwrap_err().code(), "invalidKey");
        assert_eq!(engine.plus_activate("7K2Q-AB3D-9XYZ-0000").unwrap_err().code(), "otherDevice");
        assert!(!engine.plus_active());

        let status = engine.plus_activate("7k2q ab3d 9xyz 0mnp").unwrap();
        assert_eq!((status.active, status.source), (true, Some(PlusSource::License)));
        let req = pelli.requests().last().cloned().unwrap();
        assert_eq!(req.body, json!({ "key": "7K2Q-AB3D-9XYZ-0MNP", "device": device }));
        assert!(req.authorization.is_none(), "no account involved");
        let plus_events: Vec<_> = events.named("plus");
        assert_eq!(plus_events.len(), 1);
        assert_eq!(plus_events[0]["source"], "license");
        assert_eq!(sender.events(), ["plus_activated"]);
        assert_eq!(sender.payloads()[0]["props"], json!({ "via": "key" }));

        // Checked today already (fresh from Pelli).
        let before = pelli.requests().len();
        engine.verify_license_if_due();
        assert_eq!(pelli.requests().len(), before);
        // A day later: still active, kept.
        engine.kv_set(VERIFIED_KEY, &(now_ms() - VERIFY_EVERY_MS - 1).to_string()).unwrap();
        engine.verify_license_if_due();
        assert_eq!(pelli.requests().last().unwrap().body, json!({ "license": token }));
        assert!(engine.plus_active());
        // Refunded meanwhile: removed, the UI is told.
        engine.kv_set(VERIFIED_KEY, "0").unwrap();
        *verify_status.lock() = "revoked";
        engine.verify_license_if_due();
        assert!(!engine.plus_active());
        assert_eq!(events.named("plus").last().unwrap()["active"], false);

        // Release: Pelli first, then forgotten.
        engine.plus_activate("7K2Q-AB3D-9XYZ-0MNP").unwrap();
        let status = engine.plus_release().unwrap();
        assert!(!status.active);
        assert_eq!(pelli.requests().last().unwrap().path, "/api/cull/plus/release");
        assert_eq!(engine.kv_get(LICENSE_KEY).unwrap(), None);

        // Offline: release fails, the licence stays; verify changes nothing.
        engine.plus_activate("7K2Q-AB3D-9XYZ-0MNP").unwrap();
        *engine.api_url.write() = "http://127.0.0.1:9".into();
        assert_eq!(engine.plus_release().unwrap_err().code(), "offline");
        engine.kv_set(VERIFIED_KEY, "0").unwrap();
        engine.verify_license_if_due();
        assert!(engine.plus_active());
        assert_eq!(engine.plus_activate("7K2Q-AB3D-9XYZ-0MNP").unwrap_err().code(), "offline");
        assert_eq!(engine.plus_offer("fr").unwrap(), None, "no offer offline");
    }

    #[test]
    fn checkout_poll_and_cancel() {
        let tmp = tempfile::tempdir().unwrap();
        let (engine, events, sender) = engine(tmp.path());
        let token = token_for(&engine);
        let paid = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let is_paid = paid.clone();
        let pelli = fake_pelli(move |req| match req.path.as_str() {
            "/api/cull/plus/offer?lang=fr" => (200, json!({ "amount": 2500, "currency": "eur", "label": "25 €" })),
            "/api/cull/plus/checkout" => {
                (200, json!({ "id": "co_1", "claimToken": "secret", "url": "https://checkout.stripe.com/c/1" }))
            }
            "/api/cull/plus/claim" if req.body["id"] != "co_1" => (200, json!({ "status": "expired" })),
            "/api/cull/plus/claim" if is_paid.load(std::sync::atomic::Ordering::SeqCst) => {
                (200, json!({ "status": "active", "key": "7K2Q-AB3D-9XYZ-0MNP", "license": token }))
            }
            "/api/cull/plus/claim" => (200, json!({ "status": "pending" })),
            _ => (404, json!({})),
        });
        *engine.api_url.write() = pelli.url.clone();

        let offer = engine.plus_offer("fr").unwrap().unwrap();
        assert_eq!((offer.amount, offer.currency.as_str(), offer.label.as_str()), (2500, "eur", "25 €"));
        assert!(!engine.plus_poll().unwrap().pending, "nothing to poll");

        let url = engine.plus_checkout("fr").unwrap();
        assert_eq!(url, "https://checkout.stripe.com/c/1");
        let req = pelli.requests().last().cloned().unwrap();
        assert_eq!(req.body["device"], engine.device_hash());
        assert_eq!((req.body["lang"].as_str(), req.body["platform"].as_str()), (Some("fr"), Some(crate::pelli::os())));
        assert!(engine.plus_status().pending);
        assert_eq!(events.named("plus").last().unwrap()["pending"], true);

        assert!(engine.plus_poll().unwrap().pending, "not paid yet");
        assert_eq!(pelli.requests().last().unwrap().body, json!({ "id": "co_1", "claimToken": "secret" }));
        paid.store(true, std::sync::atomic::Ordering::SeqCst);
        let status = engine.plus_poll().unwrap();
        assert_eq!((status.active, status.pending), (true, false));
        assert_eq!(status.key.as_deref(), Some("7K2Q-AB3D-9XYZ-0MNP"));
        assert_eq!(sender.payloads().last().unwrap()["props"], json!({ "via": "purchase" }));

        // An abandoned checkout is forgotten.
        engine.forget_license().unwrap();
        engine.plus_checkout("fr").unwrap();
        let status = engine.plus_cancel_checkout().unwrap();
        assert!(!status.pending && !status.active);
        // One that expired on Pelli's side too.
        engine.kv_set(CHECKOUT_KEY, &json!({ "id": "co_old", "claimToken": "x" }).to_string()).unwrap();
        assert!(!engine.plus_poll().unwrap().pending);
    }

    #[test]
    fn a_plus_account_signing_in_and_out_is_announced() {
        let tmp = tempfile::tempdir().unwrap();
        let (engine, events, sender) = engine(tmp.path());
        let plus = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let has_plus = plus.clone();
        let pelli = fake_pelli(move |req| match req.path.as_str() {
            "/api/cull/me" => {
                let p = has_plus.load(std::sync::atomic::Ordering::SeqCst);
                (
                    200,
                    json!({ "account": { "name": "C", "email": "c@example.com", "slug": "c", "avatarUrl": null,
                    "plan": if p { "pro" } else { "free" }, "planLabel": "Pro", "trialEndsAt": null,
                    "trialAvailable": false, "lifetime": false, "plus": p,
                    "entitlements": { "analyze": true, "monthlyQuota": null, "usedThisMonth": 0, "resetsAt": 0,
                                      "sendToGallery": true } } }),
                )
            }
            _ => (404, json!({})),
        });
        *engine.api_url.write() = pelli.url.clone();
        sign_in(&engine, false);
        engine.refresh_account().unwrap();
        assert_eq!(pelli.requests()[0].authorization.as_deref(), Some("Bearer bearer"));
        let status = engine.plus_status();
        assert_eq!((status.active, status.source), (true, Some(PlusSource::Account)));
        assert_eq!(events.named("plus").len(), 1);
        assert_eq!(sender.payloads()[0]["props"], json!({ "via": "account" }));
        // Same plan again: no news.
        engine.refresh_account().unwrap();
        assert_eq!(events.named("plus").len(), 1);
        // The plan lapsed.
        plus.store(false, std::sync::atomic::Ordering::SeqCst);
        engine.refresh_account().unwrap();
        assert_eq!(events.named("plus").len(), 2);
        assert!(!engine.plus_active());
        plus.store(true, std::sync::atomic::Ordering::SeqCst);
        engine.refresh_account().unwrap();
        engine.auth_logout().unwrap();
        assert_eq!(events.named("plus").last().unwrap()["active"], false);
    }
}
