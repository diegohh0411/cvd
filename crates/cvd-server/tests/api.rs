use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use cvd_core::{core, state::AppState, store::TenantStore};
use cvd_server::{router, rpc, Events};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

/// The services run the same on SQLite; Postgres tenancy is covered in cvd-core.
async fn app(
    static_dir: Option<&std::path::Path>,
) -> (Router, Arc<AppState>, Events, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = TenantStore::open(&format!("sqlite://{}", dir.path().join("cv.db").display()))
        .await
        .unwrap();
    let events = Events::default();
    let state = core::start(store, events.emitter()).await.unwrap();
    (
        router(state.clone(), events.clone(), static_dir),
        state,
        events,
        dir,
    )
}

async fn rpc(app: &Router, cmd: &str, body: Option<Value>) -> (StatusCode, Value) {
    let body = body.map_or_else(Body::empty, |b| Body::from(b.to_string()));
    let res = app
        .clone()
        .oneshot(
            Request::post(format!("/api/rpc/{cmd}"))
                .header("content-type", "application/json")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn calls_commands_with_camel_case_args() {
    let (app, _state, _events, _dir) = app(None).await;
    let (status, statuses) = rpc(&app, "list_job_statuses", None).await;
    assert_eq!(status, StatusCode::OK);
    let status_id = statuses[0]["id"].clone();
    let (status, id) = rpc(
        &app,
        "create_job",
        Some(json!({ "title": "Engineer", "company": "Acme", "rawText": "Rust", "url": "", "statusId": status_id })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, job) = rpc(&app, "get_job", Some(json!({ "id": id }))).await;
    assert_eq!(job["title"], "Engineer");
    // Omitted `Option` args are `None`, as with Tauri.
    let (status, _) = rpc(
        &app,
        "list_remote_models",
        Some(json!({ "refresh": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn errors_use_the_app_error_shape() {
    let (app, _state, _events, _dir) = app(None).await;
    let (status, body) = rpc(&app, "get_job", Some(json!({ "id": 999 }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["kind"], "NotFound", "{body}");
    assert!(body["message"].is_string());

    let (status, body) = rpc(&app, "get_job", Some(json!({ "id": "x" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["kind"], "Io");

    let (status, body) = rpc(&app, "drop_tables", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["kind"], "NotFound");
}

#[tokio::test]
async fn rejects_cross_site_and_form_posts() {
    let (app, state, _events, _dir) = app(None).await;
    cvd_core::services::set_ui_setting(&state, "ui.k", "kept")
        .await
        .unwrap();
    let cases = [
        // An HTML form: no preflight, so it must not reach the command.
        ("application/x-www-form-urlencoded", None),
        ("text/plain", None),
        ("application/json", Some("cross-site")),
        ("application/json", Some("same-site")),
    ];
    for (content_type, site) in cases {
        let mut req = Request::post("/api/rpc/set_ui_setting").header("content-type", content_type);
        if let Some(site) = site {
            req = req.header("sec-fetch-site", site);
        }
        let body = Body::from(json!({ "key": "ui.k", "value": "overwritten" }).to_string());
        let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::FORBIDDEN,
            "{content_type} {site:?}"
        );
    }
    let kept = cvd_core::services::get_ui_setting(&state, "ui.k")
        .await
        .unwrap();
    assert_eq!(kept.as_deref(), Some("kept"));

    let req = Request::post("/api/rpc/get_ui_setting")
        .header("content-type", "application/json; charset=utf-8")
        .header("sec-fetch-site", "same-origin")
        .body(Body::from(json!({ "key": "ui.k" }).to_string()))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(req).await.unwrap().status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn desktop_only_commands_explain_themselves() {
    let (app, _state, _events, _dir) = app(None).await;
    for cmd in rpc::DESKTOP_ONLY {
        let (status, body) = rpc(&app, cmd, Some(json!({ "jobId": 1 }))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{cmd}");
        assert!(
            body["message"]
                .as_str()
                .unwrap()
                .contains("not available on the web"),
            "{cmd}"
        );
    }
}

#[tokio::test]
async fn export_returns_pdf_bytes_without_a_path() {
    let (app, _state, _events, _dir) = app(None).await;
    let (status, body) = rpc(
        &app,
        "export_cv",
        Some(json!({ "runId": 1, "selection": { "entries": [], "skills": [] } })),
    )
    .await;
    // No run exists; the point is that `path` isn't required.
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["kind"], "NotFound", "{body}");
}

#[tokio::test]
async fn events_stream_carries_emitted_events() {
    let (app, _state, events, _dir) = app(None).await;
    let res = app
        .clone()
        .oneshot(Request::get("/api/events").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.headers()["content-type"], "text/event-stream");
    let mut body = res.into_body();
    (events.emitter())("db://changed", json!({ "tables": ["job_description"] }));
    let text = tokio::time::timeout(Duration::from_secs(5), async {
        let mut text = String::new();
        while text.matches("\n\n").count() < 2 {
            let frame = body.frame().await.unwrap().unwrap();
            if let Ok(data) = frame.into_data() {
                text.push_str(std::str::from_utf8(&data).unwrap());
            }
        }
        text
    })
    .await
    .unwrap();
    let mut frames = text.split("\n\n");
    let ready = frames.next().unwrap();
    assert!(ready.lines().any(|l| l == "event: ready"), "{ready}");
    let tables = ready
        .lines()
        .find_map(|l| l.strip_prefix("data: "))
        .unwrap();
    assert!(serde_json::from_str::<Value>(tables).unwrap()["tables"]
        .as_array()
        .unwrap()
        .contains(&json!("match_run")));
    let data = frames
        .next()
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("data: "))
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(data).unwrap(),
        json!({ "event": "db://changed", "payload": { "tables": ["job_description"] } })
    );
}

#[tokio::test]
async fn serves_the_web_build_with_spa_fallback() {
    let web = tempfile::tempdir().unwrap();
    std::fs::write(web.path().join("index.html"), "<app-root>").unwrap();
    std::fs::write(web.path().join("main.js"), "boot()").unwrap();
    let (app, _state, _events, _dir) = app(Some(web.path())).await;
    for (path, expected) in [
        ("/main.js", "boot()"),
        ("/", "<app-root>"),
        ("/jobs/3", "<app-root>"),
    ] {
        let res = app
            .clone()
            .oneshot(Request::get(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{path}");
        let body = res.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body, expected, "{path}");
    }
}

fn camel(snake: &str) -> String {
    let mut parts = snake.split('_');
    let mut out = parts.next().unwrap_or_default().to_owned();
    for p in parts {
        let mut c = p.chars();
        out.extend(c.next().map(|c| c.to_ascii_uppercase()));
        out.push_str(c.as_str());
    }
    out
}

/// `call<T>('name', { a, b: x })` in the frontend, as name -> argument names.
fn frontend_calls() -> BTreeMap<String, BTreeSet<String>> {
    let src = [
        include_str!("../../../apps/web/src/app/core/ipc/commands.ts"),
        include_str!("../../../apps/web/src/app/core/platform/tauri/runtime.ts"),
    ]
    .concat();
    let calls: BTreeMap<_, _> = src
        .split("call<")
        .skip(1)
        .filter_map(|s| {
            let (_, rest) = s.split_once(">('")?;
            let (name, rest) = rest.split_once('\'')?;
            let args = match rest.strip_prefix(", {") {
                Some(r) => r[..r.find('}')?]
                    .split(',')
                    .map(|a| a.split(':').next().unwrap_or_default().trim())
                    .filter(|a| !a.is_empty())
                    .map(str::to_owned)
                    .collect(),
                None => BTreeSet::new(),
            };
            Some((name.to_owned(), args))
        })
        .collect();
    assert!(calls.len() > 40, "parsed {} calls", calls.len());
    calls
}

/// Every frontend call names a registered command with the registry's camelCase
/// argument names. Shared commands expand from the registry on both transports;
/// the desktop crate holds its hand-written commands to the registry's arguments.
#[test]
fn frontend_matches_the_command_registry() {
    use cvd_core::commands::{DESKTOP_ONLY, HANDWRITTEN, PER_TRANSPORT, SHARED};
    let calls = frontend_calls();
    let registered: BTreeSet<&str> = SHARED
        .iter()
        .map(|(n, _)| *n)
        .chain(DESKTOP_ONLY.iter().copied())
        .chain(PER_TRANSPORT.iter().copied())
        .collect();
    for name in calls.keys() {
        assert!(
            registered.contains(name.as_str()),
            "{name} is not registered"
        );
    }
    for (name, args) in SHARED.iter().chain(HANDWRITTEN) {
        let want: BTreeSet<String> = args.iter().map(|a| camel(a)).collect();
        if let Some(have) = calls.get(*name) {
            assert_eq!(have, &want, "{name}");
        }
    }
}

/// Every shared and per-transport command routes; none falls through to 404.
#[tokio::test]
async fn every_registered_command_is_routable() {
    let (app, _state, _events, _dir) = app(None).await;
    for cmd in rpc::commands() {
        // Commands with arguments stop at argument parsing; skip the arg-less ones with side effects.
        if matches!(cmd, "start_scan" | "retry_watcher") {
            continue;
        }
        let (status, _) = rpc(&app, cmd, Some(json!({}))).await;
        assert_ne!(status, StatusCode::NOT_FOUND, "{cmd}");
    }
    let (status, _) = rpc(&app, "no_such_command", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// The server takes the desktop's arguments for per-transport commands.
#[tokio::test]
async fn per_transport_commands_accept_the_desktop_arguments() {
    let (app, _state, _events, _dir) = app(None).await;
    let (_, body) = rpc(
        &app,
        "export_cv",
        Some(json!({ "runId": 1, "selection": { "entries": [], "skills": [] }, "path": "/tmp/x.pdf" })),
    )
    .await;
    let message = body["message"].as_str().unwrap_or_default();
    assert!(!message.contains("invalid arguments"), "{message}");
}

mod accounts {
    use super::*;
    use cvd_core::store::{Store, UserId};
    use cvd_server::{auth::Verifier, router_with, tenants::Tenants, Access, ClientAuth};
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

    const ISSUER: &str = "https://test.supabase.co/auth/v1";
    const SECRET: &[u8] = b"test-secret-that-is-long-enough!";

    async fn app() -> (Router, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&format!("sqlite://{}", dir.path().join("cv.db").display()))
            .await
            .unwrap();
        let tenants = Arc::new(Tenants::new(store, None, Default::default()));
        (app_with(tenants), dir)
    }

    pub(super) fn app_with(tenants: Arc<Tenants>) -> Router {
        use base64::Engine;
        let keys = serde_json::from_value(json!({ "keys": [{
            "kty": "oct", "kid": "k1", "alg": "HS256",
            "k": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(SECRET),
        }]}))
        .unwrap();
        let access = Access::Accounts {
            verifier: Verifier::with_keys(ISSUER, keys),
            tenants,
            client: ClientAuth {
                supabase_url: "https://test.supabase.co".into(),
                supabase_anon_key: "anon".into(),
            },
        };
        router_with(access, None)
    }

    pub(super) fn token(user: uuid::Uuid) -> String {
        token_expiring(user, chrono::Utc::now().timestamp() + 600)
    }

    fn token_expiring(user: uuid::Uuid, exp: i64) -> String {
        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some("k1".into());
        let claims = json!({ "sub": user, "aud": "authenticated", "iss": ISSUER, "exp": exp });
        encode(&header, &claims, &EncodingKey::from_secret(SECRET)).unwrap()
    }

    pub(super) async fn send(app: &Router, req: Request<Body>) -> StatusCode {
        app.clone().oneshot(req).await.unwrap().status()
    }

    pub(super) fn call(token: Option<&str>) -> Request<Body> {
        let mut req =
            Request::post("/api/rpc/list_job_statuses").header("content-type", "application/json");
        if let Some(t) = token {
            req = req.header("authorization", format!("Bearer {t}"));
        }
        req.body(Body::empty()).unwrap()
    }

    #[tokio::test]
    async fn unauthenticated_rpc_and_events_get_401() {
        let (app, _dir) = app().await;
        assert_eq!(send(&app, call(None)).await, StatusCode::UNAUTHORIZED);
        assert_eq!(
            send(&app, call(Some("garbage"))).await,
            StatusCode::UNAUTHORIZED
        );
        let events = Request::get("/api/events").body(Body::empty()).unwrap();
        assert_eq!(send(&app, events).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn a_signed_in_user_reaches_their_services() {
        let (app, _dir) = app().await;
        let t = token(UserId::LOCAL.0);
        assert_eq!(send(&app, call(Some(&t))).await, StatusCode::OK);
        // EventSource can't set headers, so the stream takes the cookie.
        let events = Request::get("/api/events")
            .header("cookie", format!("other=1; cvd_access_token={t}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(&app, events).await, StatusCode::OK);
        // RPC never authenticates by cookie.
        let cookie_call = Request::post("/api/rpc/list_job_statuses")
            .header("content-type", "application/json")
            .header("cookie", format!("cvd_access_token={t}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(&app, cookie_call).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn the_event_stream_ends_when_its_token_expires() {
        let (app, _dir) = app().await;
        // Inside the verifier's clock-skew leeway, so accepted, but already past `exp`.
        let t = token_expiring(UserId::LOCAL.0, chrono::Utc::now().timestamp() - 5);
        let res = app
            .clone()
            .oneshot(
                Request::get("/api/events")
                    .header("authorization", format!("Bearer {t}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = tokio::time::timeout(Duration::from_secs(5), res.into_body().collect())
            .await
            .expect("stream should close at the token's expiry");
        assert!(body.is_ok());
    }

    #[tokio::test]
    async fn another_user_never_falls_back_to_the_local_data() {
        let (app, _dir) = app().await;
        // SQLite only holds the local user, so a different `sub` must fail, not share it.
        let t = token(uuid::Uuid::new_v4());
        assert_eq!(
            send(&app, call(Some(&t))).await,
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn config_tells_the_browser_where_to_sign_in() {
        let (app, _dir) = app().await;
        let res = app
            .clone()
            .oneshot(Request::get("/api/config").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body: Value =
            serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(body["auth"]["supabaseUrl"], "https://test.supabase.co");
        assert_eq!(body["auth"]["supabaseAnonKey"], "anon");
    }
}

/// Each user's services are their own, unload when idle, and stay within `max_users`.
mod residency {
    use super::accounts::{app_with, call, send, token};
    use super::*;
    use cvd_core::store::UserId;
    use cvd_server::tenants::{Residency, Starter, Tenant, TenantError, Tenants};
    use std::sync::Weak;

    /// Every user gets their own SQLite file; the services don't know the difference.
    fn starter(dir: &tempfile::TempDir) -> Starter {
        let dir = dir.path().to_owned();
        Arc::new(move |user: UserId| {
            let path = dir.join(format!("{}.db", user.0));
            Box::pin(async move {
                let store = TenantStore::open(&format!("sqlite://{}", path.display())).await?;
                let events = Events::default();
                Ok(Tenant::new(
                    core::start(store, events.emitter()).await?,
                    events,
                ))
            })
        })
    }

    fn tenants(dir: &tempfile::TempDir, max_users: usize) -> Arc<Tenants> {
        Arc::new(Tenants::with_starter(
            starter(dir),
            Residency {
                max_users,
                idle_after: Duration::ZERO,
            },
        ))
    }

    fn user() -> UserId {
        UserId(uuid::Uuid::new_v4())
    }

    #[tokio::test]
    async fn cancelling_one_users_run_leaves_anothers_running() {
        let dir = tempfile::tempdir().unwrap();
        let tenants = tenants(&dir, 10);
        let app = app_with(tenants.clone());
        let (a, b) = (user(), user());
        // Both users' first runs get id 1 in their own databases.
        let run_a = tenants.get(a).await.unwrap().state.register_run(1).unwrap();
        let run_b = tenants.get(b).await.unwrap().state.register_run(1).unwrap();

        let cancel = Request::post("/api/rpc/cancel_match")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", token(a.0)))
            .body(Body::from(json!({ "runId": 1 }).to_string()))
            .unwrap();
        assert_eq!(send(&app, cancel).await, StatusCode::OK);
        assert!(run_a.is_cancelled());
        assert!(!run_b.is_cancelled());
    }

    #[tokio::test]
    async fn idle_users_unload_but_busy_or_listening_ones_stay() {
        let dir = tempfile::tempdir().unwrap();
        let tenants = tenants(&dir, 10);
        let app = app_with(tenants.clone());
        let (idle, busy, listening) = (user(), user(), user());
        for u in [idle, busy, listening] {
            assert_eq!(send(&app, call(Some(&token(u.0)))).await, StatusCode::OK);
        }
        let _run = tenants
            .get(busy)
            .await
            .unwrap()
            .state
            .register_run(7)
            .unwrap();
        let events = app
            .clone()
            .oneshot(
                Request::get("/api/events")
                    .header("authorization", format!("Bearer {}", token(listening.0)))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(tenants.resident(), 3);
        assert_eq!(tenants.sweep(), 1);
        assert_eq!(tenants.resident(), 2);

        // The run finishing and the stream closing make both unloadable.
        tenants.get(busy).await.unwrap().state.cancel_run(7);
        cvd_core::state::remove_run(&tenants.get(busy).await.unwrap().state.runs(), 7);
        drop(events);
        assert_eq!(tenants.sweep(), 2);
        assert_eq!(tenants.resident(), 0);
    }

    #[tokio::test]
    async fn a_full_server_unloads_the_least_recently_used_idle_user() {
        let dir = tempfile::tempdir().unwrap();
        let tenants = tenants(&dir, 2);
        let app = app_with(tenants.clone());
        let (busy, idle, newcomer, late) = (user(), user(), user(), user());
        let _run = tenants
            .get(busy)
            .await
            .unwrap()
            .state
            .register_run(1)
            .unwrap();
        let idle_state = Arc::downgrade(&tenants.get(idle).await.unwrap().state);

        // `busy` is older but running, so `idle` makes room.
        assert_eq!(
            send(&app, call(Some(&token(newcomer.0)))).await,
            StatusCode::OK
        );
        assert_eq!(tenants.resident(), 2);
        assert!(tenants.get(busy).await.is_ok());
        freed(&[idle_state]).await;

        // With every loaded user busy there is no room.
        let _run2 = tenants
            .get(newcomer)
            .await
            .unwrap()
            .state
            .register_run(1)
            .unwrap();
        assert!(matches!(tenants.get(late).await, Err(TenantError::Full)));
        let res = app
            .clone()
            .oneshot(call(Some(&token(late.0))))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(res.headers()["retry-after"], "60");
    }

    /// Load test: many users pass through a small server and memory stays bounded,
    /// since unloaded users' services (mail watcher included) are actually freed.
    #[tokio::test]
    async fn many_users_stay_within_the_resident_limit() {
        let dir = tempfile::tempdir().unwrap();
        let tenants = tenants(&dir, 8);
        let app = app_with(tenants.clone());
        let mut states: Vec<Weak<AppState>> = Vec::new();
        for _ in 0..64 {
            let u = user();
            assert_eq!(send(&app, call(Some(&token(u.0)))).await, StatusCode::OK);
            states.push(Arc::downgrade(&tenants.get(u).await.unwrap().state));
            assert!(tenants.resident() <= 8);
        }
        freed(&states[..56]).await;
        assert!(states[56..].iter().all(|s| s.strong_count() > 0));
        tenants.sweep();
        freed(&states).await;
    }

    #[tokio::test]
    async fn a_running_scan_keeps_its_user_and_retiring_blocks_new_ones() {
        let dir = tempfile::tempdir().unwrap();
        let tenants = tenants(&dir, 10);
        let u = user();
        let state = tenants.get(u).await.unwrap().state.clone();
        let scan = state.begin_scan().unwrap();
        assert_eq!(tenants.sweep(), 0);
        drop(scan);
        assert_eq!(tenants.sweep(), 1);
        // The unloaded state's watcher can't start another scan before it's aborted.
        assert!(state.begin_scan().is_none());
    }

    #[tokio::test]
    async fn unloading_does_not_refill_rate_limits() {
        let dir = tempfile::tempdir().unwrap();
        let tenants = tenants(&dir, 1);
        let app = app_with(tenants.clone());
        let (a, b) = (user(), user());
        let scan = |u: UserId| {
            Request::post("/api/rpc/start_scan")
                .header("content-type", "application/json")
                .header("authorization", format!("Bearer {}", token(u.0)))
                .body(Body::empty())
                .unwrap()
        };
        // No Gmail account, so each call fails, but it still spends a token.
        for _ in 0..3 {
            assert_eq!(send(&app, scan(a)).await, StatusCode::BAD_REQUEST);
        }
        assert_eq!(send(&app, scan(a)).await, StatusCode::TOO_MANY_REQUESTS);
        // `b` takes the only slot, unloading `a`; `a` comes back still limited.
        assert_eq!(send(&app, call(Some(&token(b.0)))).await, StatusCode::OK);
        tenants.sweep();
        assert_eq!(send(&app, scan(a)).await, StatusCode::TOO_MANY_REQUESTS);
    }

    /// Unloaded states free once their aborted tasks are dropped.
    async fn freed(states: &[Weak<AppState>]) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while states.iter().any(|s| s.strong_count() > 0) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("unloaded users' services should be freed");
    }
}

#[tokio::test]
async fn expensive_commands_are_rate_limited() {
    let (app, _state, _events, _dir) = app(None).await;
    // `start_scan` allows a burst of 3; it fails without a Gmail account, which still counts.
    for _ in 0..3 {
        let (status, _) = rpc(&app, "start_scan", None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    let res = app
        .clone()
        .oneshot(
            Request::post("/api/rpc/start_scan")
                .header("content-type", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        res.headers()["retry-after"]
            .to_str()
            .unwrap()
            .parse::<u64>()
            .unwrap()
            > 0
    );
    // Other commands are unaffected.
    let (status, _) = rpc(&app, "list_job_statuses", None).await;
    assert_eq!(status, StatusCode::OK);
}
