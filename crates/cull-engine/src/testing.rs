//! Test helpers: an engine in a temporary folder, recorders of events and
//! telemetry, a fake Pelli API, licence tokens signed by a test key.

use crate::telemetry::TelemetrySender;
use crate::{Engine, EngineConfig, EventSink};
use base64::Engine as _;
use parking_lot::Mutex;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::sync::Arc;

/// Seed of the test licence key (never Pelli's).
pub(crate) const TEST_SEED: [u8; 32] = [7; 32];

/// A licence token for `payload`, signed by the test key.
pub(crate) fn sign_token(payload: &Value) -> String {
    let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let key = ring::signature::Ed25519KeyPair::from_seed_unchecked(&TEST_SEED).unwrap();
    let first = b64.encode(payload.to_string());
    let signature = key.sign(first.as_bytes());
    format!("{first}.{}", b64.encode(signature.as_ref()))
}

/// An engine without models in `dir`.
pub(crate) fn test_engine(dir: &Path, events: Arc<dyn EventSink>) -> Arc<Engine> {
    Engine::open(EngineConfig {
        data_dir: dir.join("data"),
        cache_dir: dir.join("cache"),
        models_dir: dir.join("no-models"),
        pack_dir: None,
        url_mapper: Arc::new(|p: &Path| p.display().to_string()),
        events,
        secrets: None,
    })
    .unwrap()
}

/// Every event emitted.
#[derive(Default)]
pub(crate) struct Recorder(Mutex<Vec<(String, Value)>>);

impl Recorder {
    /// Payloads of the `name` events.
    pub(crate) fn named(&self, name: &str) -> Vec<Value> {
        self.0.lock().iter().filter(|(n, _)| n == name).map(|(_, v)| v.clone()).collect()
    }
}

impl EventSink for Recorder {
    fn emit(&self, event: &str, payload: Value) {
        self.0.lock().push((event.to_string(), payload));
    }
}

/// Telemetry payloads, kept instead of sent.
#[derive(Default)]
pub(crate) struct TestSender(Mutex<Vec<Value>>);

impl TestSender {
    pub(crate) fn payloads(&self) -> Vec<Value> {
        self.0.lock().clone()
    }

    pub(crate) fn events(&self) -> Vec<String> {
        self.0.lock().iter().map(|p| p["event"].as_str().unwrap_or_default().to_string()).collect()
    }
}

impl TelemetrySender for TestSender {
    fn send(&self, payload: Value) {
        self.0.lock().push(payload);
    }
}

/// A request the fake Pelli received.
#[derive(Debug, Clone)]
pub(crate) struct Request {
    /// Path and query (`/api/cull/plus/offer?lang=fr`).
    pub(crate) path: String,
    pub(crate) authorization: Option<String>,
    /// JSON body (`null` without one).
    pub(crate) body: Value,
}

pub(crate) struct FakePelli {
    /// Its origin, for `Engine::api_url`.
    pub(crate) url: String,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl FakePelli {
    pub(crate) fn requests(&self) -> Vec<Request> {
        self.requests.lock().clone()
    }
}

/// A local HTTP server answering every request with `answer` (status, JSON
/// body), one request per connection.
pub(crate) fn fake_pelli(answer: impl Fn(&Request) -> (u16, Value) + Send + Sync + 'static) -> FakePelli {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let (seen, answer) = (requests.clone(), Arc::new(answer));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (seen, answer) = (seen.clone(), answer.clone());
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let (mut length, mut authorization) = (0usize, None);
                loop {
                    let mut h = String::new();
                    if reader.read_line(&mut h).unwrap_or(0) == 0 || h == "\r\n" {
                        break;
                    }
                    let (name, value) = h.split_once(':').unwrap_or((&h, ""));
                    match name.trim().to_ascii_lowercase().as_str() {
                        "content-length" => length = value.trim().parse().unwrap_or(0),
                        "authorization" => authorization = Some(value.trim().to_string()),
                        _ => {}
                    }
                }
                let mut body = vec![0u8; length];
                if reader.read_exact(&mut body).is_err() {
                    return;
                }
                let request =
                    Request { path, authorization, body: serde_json::from_slice(&body).unwrap_or(Value::Null) };
                let (status, reply) = answer(&request);
                seen.lock().push(request);
                let reply = reply.to_string();
                let mut out = stream;
                let _ = out.write_all(
                    format!(
                        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                        reply.len()
                    )
                    .as_bytes(),
                );
            });
        }
    });
    FakePelli { url, requests }
}
