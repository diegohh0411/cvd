//! HTTP front for the core services: RPC, server-sent events and the web build.
pub mod auth;
pub mod limits;
pub mod rpc;
pub mod tenants;

use std::{convert::Infallible, path::Path, sync::Arc, time::SystemTime};

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

use auth::{AuthError, Verifier};
use rpc::RpcError;
use tenants::{Tenant, TenantError, Tenants};

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

    /// Open event streams.
    pub fn listeners(&self) -> usize {
        self.0.receiver_count()
    }
}

/// What the browser needs to sign in; public values only.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientAuth {
    pub supabase_url: String,
    pub supabase_anon_key: String,
}

/// Who a request acts as.
#[allow(clippy::large_enum_variant)] // built once per server
pub enum Access {
    /// Every request is this user; for local development and tests.
    Open(Arc<Tenant>),
    /// Requests carry a Supabase access token; each user gets their own services.
    Accounts {
        verifier: Verifier,
        tenants: Arc<Tenants>,
        client: ClientAuth,
    },
}

/// The SSE stream reads the token from this cookie, since `EventSource` can't set headers.
pub const TOKEN_COOKIE: &str = "cvd_access_token";

impl Access {
    /// RPC calls need the `Authorization` header; the event stream also accepts the cookie.
    #[allow(clippy::result_large_err)] // the error is the response to send
    async fn tenant(
        &self,
        headers: &HeaderMap,
        allow_cookie: bool,
    ) -> Result<(Arc<Tenant>, Option<SystemTime>), Response> {
        let (verifier, tenants) = match self {
            Access::Open(t) => return Ok((t.clone(), None)),
            Access::Accounts {
                verifier, tenants, ..
            } => (verifier, tenants),
        };
        let token = bearer(headers).or_else(|| {
            allow_cookie
                .then(|| cookie(headers, TOKEN_COOKIE))
                .flatten()
        });
        let session = match token {
            None => Err(AuthError::Missing),
            Some(t) => verifier.verify(t).await,
        };
        let session = session.map_err(|e| {
            let status = match e {
                AuthError::Keys(_) => StatusCode::SERVICE_UNAVAILABLE,
                _ => StatusCode::UNAUTHORIZED,
            };
            (status, Json(AppError::Io(e.to_string()))).into_response()
        })?;
        let tenant = tenants.get(session.user).await.map_err(|e| match e {
            TenantError::Full => {
                tracing::warn!(user = %session.user.0, "no room to load another user");
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    [(header::RETRY_AFTER, "60")],
                    Json(AppError::Io(e.to_string())),
                )
                    .into_response()
            }
            TenantError::Start(e) => {
                tracing::error!(user = %session.user.0, error = %e, "couldn't start user services");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(e)).into_response()
            }
        })?;
        Ok((tenant, Some(session.expires_at)))
    }
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then(|| token.trim())
        .filter(|t| !t.is_empty())
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, v)| *k == name && !v.is_empty())
        .map(|(_, v)| v)
}

#[derive(Clone)]
struct Ctx {
    access: Arc<Access>,
}

/// A single-user router; see [`router_with`].
pub fn router(state: Arc<AppState>, events: Events, static_dir: Option<&Path>) -> Router {
    router_with(
        Access::Open(Arc::new(Tenant::new(state, events))),
        static_dir,
    )
}

/// `static_dir` is the web build (`apps/web/dist/cvd/browser`); unknown paths serve its index.
pub fn router_with(access: Access, static_dir: Option<&Path>) -> Router {
    let api = Router::new()
        .route("/api/config", get(config))
        .route("/api/rpc/{command}", post(call))
        .route("/api/events", get(events_stream))
        .with_state(Ctx {
            access: Arc::new(access),
        });
    match static_dir {
        Some(dir) => api
            .fallback_service(ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")))),
        None => api,
    }
}

/// Tells the web build whether to show sign-in, and where.
async fn config(State(ctx): State<Ctx>) -> Json<Value> {
    let auth = match &*ctx.access {
        Access::Open(_) => None,
        Access::Accounts { client, .. } => Some(client),
    };
    Json(json!({ "auth": auth }))
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
    let tenant = match ctx.access.tenant(&headers, false).await {
        Ok((t, _)) => t,
        Err(res) => return res,
    };
    if let Err(wait) = tenant.limits.check(&command) {
        let secs = wait.as_secs().max(1);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, secs.to_string())],
            Json(AppError::Io(format!(
                "Too many {command} calls; try again in {secs} s."
            ))),
        )
            .into_response();
    }
    let args = if body.is_empty() {
        Ok(Value::Null)
    } else {
        serde_json::from_slice(&body)
    };
    let result = match args {
        Ok(args) => rpc::dispatch(&tenant.state, &command, args).await,
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

async fn events_stream(State(ctx): State<Ctx>, headers: HeaderMap) -> Response {
    match ctx.access.tenant(&headers, true).await {
        Ok((tenant, expires_at)) => stream(&tenant.events, expires_at).into_response(),
        Err(res) => res,
    }
}

/// Ends when the token expires; `EventSource` reconnects with the refreshed cookie.
fn stream(
    events: &Events,
    expires_at: Option<SystemTime>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Subscribe before `ready` so anything the client starts afterwards is delivered.
    let events = BroadcastStream::new(events.0.subscribe());
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
    let deadline = async move {
        match expires_at {
            Some(t) => {
                let left = t.duration_since(SystemTime::now()).unwrap_or_default();
                tokio::time::sleep(left).await
            }
            None => std::future::pending().await,
        }
    };
    let stream = futures_util::StreamExt::take_until(stream, Box::pin(deadline));
    Sse::new(stream).keep_alive(KeepAlive::default())
}
