//! Low-level request to Pelli's upload service — exactly what the dashboard
//! uploader sends (see `docs/pelli-api.md`, "The upload request"):
//!
//! ```http
//! POST {uploadUrl}?name=<file name>&folder=<folder id>
//! Authorization: <raw upload token>          (no "Bearer ")
//! multipart/form-data, one field `file`
//! ```
//! The gallery comes from the token. Success = 2xx with `{ "success": true }`.

use crate::pelli::PelliClient;
use crate::{EngineError, Result};
use parking_lot::RwLock;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde::Deserialize;
use std::time::Duration;

/// One-file `multipart/form-data` body: `(Content-Type header, body)`.
fn multipart(field: &str, filename: &str, content_type: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let boundary = format!("----PelliInstant{nanos:x}");
    let safe: String = filename.chars().map(|c| if c == '"' || c.is_control() { '_' } else { c }).collect();
    let mut body = Vec::with_capacity(bytes.len() + 256);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{safe}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

#[derive(Debug, Clone)]
struct Grant {
    upload_url: String,
    token: String,
    expires_at: i64,
}

/// Upload authorisation for one gallery, renewed when it expires or the
/// service refuses it.
pub(crate) struct UploadTarget {
    gallery_id: String,
    grant: RwLock<Grant>,
    pub(crate) max_file_size: Option<u64>,
    pub(crate) max_concurrency: usize,
    /// Free space reported by Pelli (`limitBytes - usedBytes`), if known.
    pub(crate) storage_free: Option<u64>,
    agent: ureq::Agent,
}

#[derive(Deserialize)]
struct UploadResponse {
    success: Option<bool>,
    code: Option<String>,
    error: Option<String>,
    message: Option<String>,
}

fn remote(code: &str, message: impl Into<String>) -> EngineError {
    EngineError::Remote { code: code.to_string(), message: message.into() }
}

/// True when the request failed before the service could receive a
/// complete photo.
fn nothing_sent(e: &ureq::Error) -> bool {
    use std::io::ErrorKind;
    match e {
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed | ureq::Error::Tls(_) => true,
        ureq::Error::Timeout(t) => matches!(
            t,
            ureq::Timeout::Resolve | ureq::Timeout::Connect | ureq::Timeout::SendRequest | ureq::Timeout::SendBody
        ),
        ureq::Error::Io(io) => matches!(
            io.kind(),
            ErrorKind::ConnectionRefused
                | ErrorKind::NotConnected
                | ErrorKind::AddrNotAvailable
                | ErrorKind::HostUnreachable
                | ErrorKind::NetworkUnreachable
        ),
        _ => false,
    }
}

fn fetch_grant(client: &PelliClient, gallery_id: &str) -> Result<(Grant, serde_json::Value)> {
    let v = client.upload_token(gallery_id)?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_owned);
    let token = s("uploadToken").ok_or_else(|| remote("bad_response", "uploadToken"))?;
    let upload_url = s("uploadUrl").ok_or_else(|| remote("bad_response", "uploadUrl"))?;
    let expires_at = v
        .get("expiresAt")
        .and_then(|x| x.as_i64())
        .unwrap_or_else(|| crate::now_ms() + v.get("expiresIn").and_then(|x| x.as_i64()).unwrap_or(3600) * 1000);
    Ok((Grant { upload_url, token, expires_at }, v))
}

/// Outcome of one attempt.
enum Attempt {
    Done,
    /// Token refused: renew and retry once.
    Unauthorized,
    /// Transient (408, 429, 5xx, connection refused before sending).
    Retry(EngineError),
    Fatal(EngineError),
}

impl UploadTarget {
    pub(crate) fn fetch(client: &PelliClient, gallery_id: &str) -> Result<UploadTarget> {
        let (grant, v) = fetch_grant(client, gallery_id)?;
        let storage_free = v.get("storage").and_then(|s| {
            let used = s.get("usedBytes")?.as_u64()?;
            let limit = s.get("limitBytes")?.as_u64()?;
            Some(limit.saturating_sub(used))
        });
        Ok(UploadTarget {
            gallery_id: gallery_id.to_string(),
            grant: RwLock::new(grant),
            max_file_size: v.get("maxFileSize").and_then(|x| x.as_u64()),
            max_concurrency: v.get("maxConcurrency").and_then(|x| x.as_u64()).unwrap_or(3).clamp(1, 3) as usize,
            storage_free,
            agent: crate::pelli::agent(Duration::from_secs(300)),
        })
    }

    fn renew(&self, client: &PelliClient) -> Result<()> {
        let (grant, _) = fetch_grant(client, &self.gallery_id)?;
        *self.grant.write() = grant;
        Ok(())
    }

    fn attempt(&self, folder_id: Option<&str>, file_name: &str, body: &[u8], content_type: &str) -> Attempt {
        let grant = self.grant.read().clone();
        let mut url = format!("{}?name={}", grant.upload_url, utf8_percent_encode(file_name, NON_ALPHANUMERIC));
        if let Some(f) = folder_id {
            url.push_str(&format!("&folder={}", utf8_percent_encode(f, NON_ALPHANUMERIC)));
        }
        let resp = match self
            .agent
            .post(&url)
            .header("Authorization", grant.token.as_str())
            .header("Content-Type", content_type)
            .send(body)
        {
            Ok(r) => r,
            // Nothing reached the service (DNS, refused connection, TLS,
            // body not fully sent): safe to retry.
            Err(e) if nothing_sent(&e) => return Attempt::Retry(remote("network", e.to_string())),
            // The whole body may have been received and the photo created:
            // resending could duplicate it (docs/pelli-api.md). Count it as
            // failed; it is not remembered as sent, so sending again retries it.
            Err(e) => return Attempt::Fatal(remote("upload_unconfirmed", e.to_string())),
        };
        let status = resp.status().as_u16();
        let mut resp = resp;
        let parsed: Option<UploadResponse> = resp
            .body_mut()
            .with_config()
            .limit(1024 * 1024)
            .read_to_string()
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok());
        let code = parsed
            .as_ref()
            .and_then(|p| p.code.clone().or_else(|| p.error.clone()))
            .unwrap_or_else(|| format!("http_{status}"));
        let message = parsed.as_ref().and_then(|p| p.message.clone()).unwrap_or_else(|| format!("HTTP {status}"));
        match status {
            200..=299 if parsed.as_ref().and_then(|p| p.success) == Some(true) => Attempt::Done,
            200..=299 => Attempt::Retry(remote("bad_response", "upload not confirmed")),
            401 => Attempt::Unauthorized,
            408 | 429 | 500..=599 => Attempt::Retry(remote(&code, message)),
            _ => Attempt::Fatal(remote(&code, message)),
        }
    }

    /// Upload one JPEG, renewing the token once and retrying transient
    /// failures after 2 s, 5 s and 15 s.
    pub(crate) fn upload(
        &self,
        client: &PelliClient,
        folder_id: Option<&str>,
        file_name: &str,
        bytes: Vec<u8>,
    ) -> Result<()> {
        if let Some(max) = self.max_file_size {
            if bytes.len() as u64 > max {
                return Err(remote("file_too_large", file_name));
            }
        }
        if self.grant.read().expires_at < crate::now_ms() + 60_000 {
            self.renew(client)?;
        }
        let (content_type, body) = multipart("file", file_name, "image/jpeg", &bytes);
        let mut renewed = false;
        let mut delays = [2u64, 5, 15].into_iter();
        loop {
            match self.attempt(folder_id, file_name, &body, &content_type) {
                Attempt::Done => return Ok(()),
                Attempt::Unauthorized if !renewed => {
                    renewed = true;
                    self.renew(client)?;
                }
                Attempt::Unauthorized => return Err(remote("unauthorized", "upload token refused")),
                Attempt::Fatal(e) => return Err(e),
                Attempt::Retry(e) => match delays.next() {
                    Some(d) => std::thread::sleep(Duration::from_secs(d)),
                    None => return Err(e),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multipart_layout() {
        let (ct, body) = multipart("f", "x\".jpg", "image/jpeg", b"\xff\xd8");
        let boundary = ct.split("boundary=").nth(1).unwrap();
        let s = String::from_utf8_lossy(&body);
        assert!(s.starts_with(&format!("--{boundary}\r\n")));
        assert!(s.contains("filename=\"x_.jpg\""));
        assert!(s.ends_with(&format!("--{boundary}--\r\n")));
    }
}
