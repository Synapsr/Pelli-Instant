//! Text safe to keep in a log file, store in the library or show for the
//! photographer to copy to support: no user name, no credential.
//!
//! [`redact`] replaces
//! - the home folder (`$HOME`, `%USERPROFILE%`) and any user folder
//!   (`/Users/<name>`, `/home/<name>`, `C:\Users\<name>`) by `~`;
//! - the value after `Bearer ` (an `Authorization` header);
//! - the string value of JSON fields that hold a secret (`license`, `token`,
//!   `key`, `authorization`, `claimToken`, `password`);
//! - the query of an `http(s)://` address (presigned links carry a
//!   signature).

use std::path::PathBuf;

const REDACTED: &str = "<redacted>";
/// JSON fields whose string value is a secret (compared ignoring case).
const SECRET_FIELDS: &[&str] = &["license", "token", "key", "authorization", "claimtoken", "password"];

/// `text` without user name nor credential (see the module docs).
pub fn redact(text: &str) -> String {
    let out = redact_home(text);
    let out = redact_user_folders(&out);
    let out = redact_bearer(&out);
    let out = redact_json_secrets(&out);
    redact_queries(&out)
}

/// The home folder written `~`.
fn redact_home(text: &str) -> String {
    let homes = ["HOME", "USERPROFILE"].into_iter().filter_map(std::env::var_os).map(PathBuf::from);
    let mut out = text.to_string();
    for home in homes {
        let home = home.to_string_lossy().trim_end_matches(['/', '\\']).to_string();
        // Not `/` nor a bare drive: those hold no user name.
        if home.len() > 3 {
            out = out.replace(&home, "~").replace(&home.replace('\\', "/"), "~");
        }
    }
    out
}

/// Any other user folder (another account's home, a path written before a
/// rename): everything up to the next separator, spaces included.
fn redact_user_folders(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let found = ["/Users/", "/home/", "\\Users\\", "\\users\\"]
            .iter()
            .filter_map(|p| rest.find(p).map(|i| (i, p.len())))
            .min();
        let Some((i, len)) = found else {
            out.push_str(rest);
            return out;
        };
        // `C:\Users\…`: the drive goes with the folder.
        let start = if i >= 2 && rest.as_bytes()[i - 1] == b':' { i - 2 } else { i };
        out.push_str(&rest[..start]);
        let after = &rest[i + len..];
        let end = after.find(['/', '\\', '\n', '"', '\'']).unwrap_or(after.len());
        out.push('~');
        rest = &after[end..];
    }
}

/// `Bearer <token>` → `Bearer <redacted>`.
fn redact_bearer(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("Bearer ") {
        out.push_str(&rest[..i + 7]);
        let after = &rest[i + 7..];
        let end = after.find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',').unwrap_or(after.len());
        if end > 0 {
            out.push_str(REDACTED);
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// `"license": "…"` → `"license": "<redacted>"` (and the other secret fields).
fn redact_json_secrets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut copied = 0;
    while i < bytes.len() {
        if bytes[i] != b'"' {
            i += 1;
            continue;
        }
        // A quoted name followed by `:` and a quoted value.
        let Some(close) = text[i + 1..].find('"').map(|k| i + 1 + k) else { break };
        let name = &text[i + 1..close];
        let mut j = close + 1;
        while j < bytes.len() && bytes[j] == b' ' {
            j += 1;
        }
        let secret = SECRET_FIELDS.iter().any(|f| name.eq_ignore_ascii_case(f));
        if secret && j < bytes.len() && bytes[j] == b':' {
            j += 1;
            while j < bytes.len() && bytes[j] == b' ' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'"' {
                // The value, escaped quotes included.
                let mut k = j + 1;
                while k < bytes.len() && bytes[k] != b'"' {
                    k += if bytes[k] == b'\\' { 2 } else { 1 };
                }
                out.push_str(&text[copied..=j]);
                out.push_str(REDACTED);
                copied = k.min(bytes.len());
                i = k + 1;
                continue;
            }
        }
        i = close + 1;
    }
    out.push_str(&text[copied.min(text.len())..]);
    out
}

/// `https://host/path?query` → `https://host/path?<redacted>`.
fn redact_queries(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let found = ["https://", "http://"].iter().filter_map(|p| rest.find(p)).min();
        let Some(i) = found else {
            out.push_str(rest);
            return out;
        };
        // An address ends at a space, a quote or a closing bracket.
        let url_end =
            rest[i..].find(|c: char| c.is_whitespace() || "\"'()<>[]{},".contains(c)).map_or(rest.len(), |e| i + e);
        let url = &rest[i..url_end];
        out.push_str(&rest[..i]);
        match url.find('?') {
            Some(q) => {
                out.push_str(&url[..=q]);
                out.push_str(REDACTED);
            }
            None => out.push_str(url),
        }
        rest = &rest[url_end..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_folders_become_tilde() {
        assert_eq!(
            redact("/Users/camille/Library/Application Support/io.pelli.instant/models/maximum/a.onnx: denied"),
            "~/Library/Application Support/io.pelli.instant/models/maximum/a.onnx: denied"
        );
        assert_eq!(redact(r"C:\Users\Camille Dupont\AppData\Roaming\x.onnx"), r"~\AppData\Roaming\x.onnx");
        assert_eq!(redact("/home/camille/.local/share/x"), "~/.local/share/x");
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            let path = PathBuf::from(&home).join("data").join("x.onnx");
            let text = redact(&format!("{}: permission denied", path.display()));
            assert!(!text.contains(&*home.to_string_lossy()) && text.starts_with('~'), "{text}");
        }
        assert_eq!(redact("dinov2-b14-fp32: wrong results"), "dinov2-b14-fp32: wrong results");
    }

    #[test]
    fn credentials_never_remain() {
        assert_eq!(redact("Authorization: Bearer abc.def-123 sent"), "Authorization: Bearer <redacted> sent");
        assert_eq!(
            redact(r#"{"event":"app_opened","license":"eyJ2Ijox\"fake","arch":"x64","Token": "t0k"}"#),
            r#"{"event":"app_opened","license":"<redacted>","arch":"x64","Token": "<redacted>"}"#
        );
        assert_eq!(
            redact("GET https://s3.example/models/a.onnx?X-Amz-Signature=abc&X-Amz-Credential=k failed"),
            "GET https://s3.example/models/a.onnx?<redacted> failed"
        );
        assert_eq!(redact(r#"{"key": 5, "model": "x"}"#), r#"{"key": 5, "model": "x"}"#, "only string values");
        assert_eq!(redact("an unterminated \"quote"), "an unterminated \"quote");
    }
}
