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
