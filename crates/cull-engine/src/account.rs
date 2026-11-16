//! The optional Pelli account: browser consent flow, cached account. Sorting
//! never needs it; a plan that includes Plus (Pro, Ultra) unlocks Pelli
//! Instant Plus ([`crate::plus`]), and sending to a Pelli gallery uses it.
//!
//! The access token (30 days) is kept by a [`SecretStore`]: the desktop host
//! plugs the OS keychain; the default stores it in the local library.

use crate::pelli::{Account, GallerySummary, PelliClient};
use crate::plus::Via;
use crate::{now_ms, Engine, EngineError, Result};
use serde::{Deserialize, Serialize};

const TOKEN_KEY: &str = "pelli.token";
const ACCOUNT_KEY: &str = "pelli.account";
const PENDING_KEY: &str = "pelli.pending";

/// True for the errors after which the app signs out (the UI's
/// `unauthorized`): access token missing, expired or revoked (401), or
/// account banned, archived or pending review (403 `account_unavailable`).
fn signs_out(e: &EngineError) -> bool {
    matches!(e, EngineError::Remote { code, .. }
        if matches!(code.as_str(), "unauthorized" | "missing_token" | "invalid_token" | "account_unavailable"))
}

/// Where the access token lives.
pub trait SecretStore: Send + Sync {
    fn get(&self) -> Option<String>;
    fn set(&self, token: &str) -> Result<()>;
    fn delete(&self) -> Result<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum AuthState {
    SignedOut,
    #[serde(rename_all = "camelCase")]
    Pending {
        auth_url: String,
        expires_at: i64,
    },
    SignedIn {
        account: Account,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pending {
    session_id: String,
    auth_url: String,
    expires_at: i64,
}

impl Engine {
    pub(crate) fn token(&self) -> Option<String> {
        match &self.secrets {
            Some(s) => s.get(),
            None => self.kv_get(TOKEN_KEY).ok().flatten(),
        }
    }

    fn set_token(&self, token: &str) -> Result<()> {
        match &self.secrets {
            Some(s) => s.set(token),
            None => self.kv_set(TOKEN_KEY, token),
        }
    }

    fn delete_token(&self) -> Result<()> {
        match &self.secrets {
            Some(s) => s.delete(),
            None => self.kv_delete(TOKEN_KEY),
        }
    }

    pub(crate) fn cached_account(&self) -> Option<Account> {
        self.kv_get(ACCOUNT_KEY).ok().flatten().and_then(|j| serde_json::from_str(&j).ok())
    }

    fn emit_auth(&self, state: &AuthState) {
        if let Ok(v) = serde_json::to_value(state) {
            self.events().emit("auth-changed", v);
        }
    }

    /// Current state without network.
    pub fn auth_status(&self) -> AuthState {
        if self.token().is_some() {
            if let Some(account) = self.cached_account() {
                return AuthState::SignedIn { account };
            }
        }
        if let Some(p) = self
            .kv_get(PENDING_KEY)
            .ok()
            .flatten()
            .and_then(|j| serde_json::from_str::<Pending>(&j).ok())
            .filter(|p| p.expires_at > now_ms())
        {
            return AuthState::Pending { auth_url: p.auth_url, expires_at: p.expires_at };
        }
        AuthState::SignedOut
    }

    /// Start the browser consent. The host opens `auth_url`.
    pub fn auth_start(&self, device_name: &str) -> Result<AuthState> {
        let s = self.pelli(None).create_session(device_name)?;
        let pending = Pending {
            session_id: s.session_id,
            auth_url: s.auth_url.clone(),
            expires_at: now_ms() + (s.expires_in as i64) * 1000,
        };
        self.kv_set(PENDING_KEY, &serde_json::to_string(&pending)?)?;
        Ok(AuthState::Pending { auth_url: pending.auth_url, expires_at: pending.expires_at })
    }

    pub fn auth_poll(&self) -> Result<AuthState> {
        let Some(p) = self.kv_get(PENDING_KEY)?.and_then(|j| serde_json::from_str::<Pending>(&j).ok()) else {
            return Ok(self.auth_status());
        };
        let check = self.pelli(None).check_session(&p.session_id)?;
        let plus_before = self.plus_status();
        let state = match check.status.as_str() {
            "approved" => {
                let token = check
                    .access_token
                    .ok_or_else(|| EngineError::Remote { code: "bad_response".into(), message: "no token".into() })?;
                self.set_token(&token)?;
                let account = match check.account {
                    Some(a) => a,
                    None => self.pelli(Some(token)).me()?,
                };
                self.kv_set(ACCOUNT_KEY, &serde_json::to_string(&account)?)?;
                self.kv_delete(PENDING_KEY)?;
                AuthState::SignedIn { account }
            }
            "pending" if p.expires_at > now_ms() => {
                return Ok(AuthState::Pending { auth_url: p.auth_url, expires_at: p.expires_at })
            }
            _ => {
                // denied / expired
                self.kv_delete(PENDING_KEY)?;
                AuthState::SignedOut
            }
        };
        self.emit_auth(&state);
        self.plus_changed(&plus_before, Via::Account);
        Ok(state)
    }

    pub fn auth_logout(&self) -> Result<AuthState> {
        let plus_before = self.plus_status();
        self.delete_token()?;
        self.kv_delete(ACCOUNT_KEY)?;
        self.kv_delete(PENDING_KEY)?;
        let state = AuthState::SignedOut;
        self.emit_auth(&state);
        self.plus_changed(&plus_before, Via::Account);
        Ok(state)
    }

    /// Refresh the account (its plan may include Plus now, or no longer).
    /// Offline: keep the cached account.
    pub fn refresh_account(&self) -> Result<AuthState> {
        let Some(token) = self.token() else { return Ok(AuthState::SignedOut) };
        match self.pelli(Some(token)).me() {
            Ok(account) => {
                let plus_before = self.plus_status();
                self.kv_set(ACCOUNT_KEY, &serde_json::to_string(&account)?)?;
                let state = AuthState::SignedIn { account };
                self.emit_auth(&state);
                self.plus_changed(&plus_before, Via::Account);
                Ok(state)
            }
            Err(e) if signs_out(&e) => self.auth_logout(),
            // Offline, server down or older Pelli: keep the cached account.
            Err(e) => {
                log::warn!("account refresh failed: {e}");
                Ok(self.auth_status())
            }
        }
    }

    /// Client of the Pelli API, in the app language.
    pub(crate) fn pelli(&self, token: Option<String>) -> PelliClient {
        PelliClient::new(&self.api_url.read(), token, &self.language())
    }

    pub(crate) fn signed_in_client(&self) -> Result<PelliClient> {
        self.token()
            .map(|t| self.pelli(Some(t)))
            .ok_or_else(|| EngineError::Remote { code: "signed_out".into(), message: "not signed in".into() })
    }

    pub fn list_galleries(&self) -> Result<Vec<GallerySummary>> {
        self.signed_in_client()?.list_galleries()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_state_json() {
        let s = serde_json::to_value(AuthState::Pending { auth_url: "u".into(), expires_at: 3 }).unwrap();
        assert_eq!(s, serde_json::json!({ "status": "pending", "authUrl": "u", "expiresAt": 3 }));
        let s = serde_json::to_value(AuthState::SignedOut).unwrap();
        assert_eq!(s, serde_json::json!({ "status": "signedOut" }));
    }
}
