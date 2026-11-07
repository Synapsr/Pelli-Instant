//! HTTP dev bridge: the real engine behind the UI running in a browser
//! (`VITE_BRIDGE_URL=http://127.0.0.1:4317 pnpm dev:bridge`).
//!
//! - `POST /invoke/{command}` — body = command args (JSON), response = JSON
//!   result, or `{ "error": { code, message } }` with status 400/500.
//! - `GET /events` — Server-Sent Events (`event:` name, `data:` JSON).
//! - `GET /files?path=…` — previews and originals, restricted to the cache
//!   and to folders the engine opened.
//!
//! Development only: binds 127.0.0.1.

use crate::Args;
use axum::extract::{Path as AxPath, Query, State};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use cull_engine::commands::{dispatch, error_json, Host};
use cull_engine::{Engine, EventSink, Result as EngineResult};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

struct BroadcastSink(broadcast::Sender<(String, Value)>);

impl EventSink for BroadcastSink {
    fn emit(&self, event: &str, payload: Value) {
        let _ = self.0.send((event.to_string(), payload));
    }
}

struct BridgeHost {
    allowed: RwLock<Vec<PathBuf>>,
}

impl Host for BridgeHost {
    fn pick_folder(&self) -> Option<PathBuf> {
        // No native dialog from a terminal process: use a configured folder.
        std::env::var("PELLI_CULL_PICK").ok().map(PathBuf::from)
    }
    fn reveal(&self, path: &Path) -> EngineResult<()> {
        log::info!("reveal {}", path.display());
        Ok(())
    }
    fn open_external(&self, url: &str) -> EngineResult<()> {
        log::info!("open {url}");
        Ok(())
    }
    fn device_name(&self) -> String {
        std::env::var("HOSTNAME").unwrap_or_else(|_| "Pelli Instant (dev)".into())
    }
    fn allow_directory(&self, dir: &Path) {
        if let Ok(c) = std::fs::canonicalize(dir) {
            let mut a = self.allowed.write().unwrap_or_else(|e| e.into_inner());
            if !a.contains(&c) {
                a.push(c);
            }
        }
    }
}

#[derive(Clone)]
struct AppState {
    engine: Arc<Engine>,
    host: Arc<BridgeHost>,
    events: broadcast::Sender<(String, Value)>,
    cache_dir: PathBuf,
}

pub fn serve(args: &Args) {
    let port: u16 = args.value("port").and_then(|p| p.parse().ok()).unwrap_or(4317);
    let (tx, _) = broadcast::channel(4096);
    let base = format!("http://127.0.0.1:{port}");
    let mapper: cull_engine::UrlMapper = Arc::new(move |p: &Path| {
        format!("{base}/files?path={}", utf8_percent_encode(&p.to_string_lossy(), NON_ALPHANUMERIC))
    });
    let engine = crate::engine(args, mapper, Arc::new(BroadcastSink(tx.clone())), None);
    let host = Arc::new(BridgeHost { allowed: RwLock::new(Vec::new()) });
    let cache_dir = std::fs::canonicalize(engine.cache_dir()).unwrap_or_else(|_| engine.cache_dir().to_path_buf());
    let state = AppState { engine, host, events: tx, cache_dir };

    let app = Router::new()
        .route("/invoke/{command}", post(invoke))
        .route("/events", get(events))
        .route("/files", get(files))
        .with_state(state)
        .layer(tower_http::cors::CorsLayer::permissive());

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async move {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await.expect("bind");
        log::info!("Pelli Instant dev bridge on http://127.0.0.1:{port}");
        axum::serve(listener, app).await.expect("server");
    });
}

async fn invoke(State(state): State<AppState>, AxPath(command): AxPath<String>, body: Option<Json<Value>>) -> Response {
    let args = body.map(|b| b.0).unwrap_or(Value::Null);
    let result =
        tokio::task::spawn_blocking(move || dispatch(&state.engine, state.host.as_ref(), &command, &args)).await;
    match result {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => {
            let status = match e.code() {
                "notFound" => StatusCode::NOT_FOUND,
                "internal" => StatusCode::INTERNAL_SERVER_ERROR,
                _ => StatusCode::BAD_REQUEST,
            };
            (status, Json(json!({ "error": error_json(&e) }))).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": { "code": "internal", "message": e.to_string() } })),
        )
            .into_response(),
    }
}

async fn events(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = std::result::Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(state.events.subscribe())
        .filter_map(|msg| msg.ok().map(|(name, payload)| Ok(Event::default().event(name).data(payload.to_string()))));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn files(State(state): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(path) = q.get("path") else { return StatusCode::BAD_REQUEST.into_response() };
    let Ok(path) = std::fs::canonicalize(path) else { return StatusCode::NOT_FOUND.into_response() };
    let allowed = path.starts_with(&state.cache_dir)
        || state.host.allowed.read().map(|a| a.iter().any(|d| path.starts_with(d))).unwrap_or(false);
    if !allowed {
        return StatusCode::FORBIDDEN.into_response();
    }
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let mime = match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
                Some("jpg" | "jpeg") => "image/jpeg",
                Some("png") => "image/png",
                Some("tif" | "tiff") => "image/tiff",
                _ => "application/octet-stream",
            };
            ([(header::CONTENT_TYPE, mime), (header::CACHE_CONTROL, "max-age=3600")], bytes).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
