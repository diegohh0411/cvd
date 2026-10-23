//! HTTP front for the core services: RPC, server-sent events and the web build.
pub mod rpc;

use std::{convert::Infallible, path::Path, sync::Arc};

use axum::{
    body::Bytes,
    extract::{Path as UrlPath, State},
    http::{header, HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use cvd_core::{
    services::AppError,
    state::{AppState, Emitter, DB_CHANGED_EVENT},
    store::CHANGE_TABLES,
};
use serde_json::{json, Value};
use tokio::sync::broadcast;
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use tower_http::services::{ServeDir, ServeFile};

use rpc::RpcError;

/// One emitted event, as sent on `/api/events`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Message {
    pub event: String,
    pub payload: Value,
}

/// Fans the core `Emitter` out to every open event stream.
#[derive(Clone)]
pub struct Events(broadcast::Sender<Message>);
impl Default for Events {
    fn default() -> Self {
        Self(broadcast::channel(256).0)
    }
}
impl Events {
    pub fn emitter(&self) -> Emitter {
        let tx = self.0.clone();
        Arc::new(move |event: &str, payload: Value| {
            // No open streams is fine: the UI refetches when it connects.
            let _ = tx.send(Message {
                event: event.to_owned(),
                payload,
            });
        })
    }
}

#[derive(Clone)]
struct Ctx {
    state: Arc<AppState>,
    events: Events,
}

/// `static_dir` is the web build (`apps/web/dist/cvd/browser`); unknown paths serve its index.
pub fn router(state: Arc<AppState>, events: Events, static_dir: Option<&Path>) -> Router {
    let api = Router::new()
        .route("/api/rpc/{command}", post(call))
        .route("/api/events", get(events_stream))
        .with_state(Ctx { state, events });
    match static_dir {
        Some(dir) => api
            .fallback_service(ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")))),
        None => api,
    }
}

/// Browsers can't send `application/json` cross-site without a CORS preflight, which this
/// server never grants, and `Sec-Fetch-Site` flags the rest. Forms and no-cors fetches fail here.
fn same_origin_json(headers: &HeaderMap) -> bool {
    let json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(';').next().unwrap_or("").trim() == "application/json");
    let site = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok());
    json && matches!(site, None | Some("same-origin") | Some("none"))
}

async fn call(
    State(ctx): State<Ctx>,
    UrlPath(command): UrlPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if !same_origin_json(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(AppError::Io(
                "RPC needs a same-origin application/json request.".into(),
            )),
        )
            .into_response();
    }
    let args = if body.is_empty() {
        Ok(Value::Null)
    } else {
        serde_json::from_slice(&body)
    };
    let result = match args {
        Ok(args) => rpc::dispatch(&ctx.state, &command, args).await,
        Err(e) => Err(AppError::Io(format!("invalid JSON body: {e}")).into()),
    };
    match result {
        Ok(v) => Json(v).into_response(),
        // Same `{ kind, message }` body Tauri rejects `invoke` with.
        Err(RpcError::App(e)) => (StatusCode::BAD_REQUEST, Json(e)).into_response(),
        Err(RpcError::Unknown(cmd)) => (
            StatusCode::NOT_FOUND,
            Json(AppError::NotFound(format!("Unknown command: {cmd}"))),
        )
            .into_response(),
    }
}

async fn events_stream(
    State(ctx): State<Ctx>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Subscribe before `ready` so anything the client starts afterwards is delivered.
    let events = BroadcastStream::new(ctx.events.0.subscribe());
    // Lists every table so a reconnecting client can refetch what it missed while away.
    let ready = Event::default()
        .event("ready")
        .json_data(json!({ "tables": CHANGE_TABLES }))
        .ok()
        .map(Ok);
    let stream =
        tokio_stream::iter(ready).chain(events.filter_map(|m| {
            match m {
        Ok(m) => Event::default().json_data(m).ok().map(Ok),
        // A slow client missed events; tell it to refetch everything.
        Err(_) => Event::default()
            .json_data(json!({ "event": DB_CHANGED_EVENT, "payload": { "tables": CHANGE_TABLES } }))
            .ok()
            .map(Ok),
    }
        }));
    Sse::new(stream).keep_alive(KeepAlive::default())
}
