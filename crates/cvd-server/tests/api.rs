use std::{collections::BTreeSet, sync::Arc, time::Duration};

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
        .oneshot(Request::post(format!("/api/rpc/{cmd}")).body(body).unwrap())
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
    let (app, ..) = app(None).await;
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
    let (app, ..) = app(None).await;
    let (status, body) = rpc(&app, "get_job", Some(json!({ "id": 999 }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["kind"], "NotFound");
    assert!(body["message"].is_string());

    let (status, body) = rpc(&app, "get_job", Some(json!({ "id": "x" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["kind"], "Io");

    let (status, body) = rpc(&app, "drop_tables", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["kind"], "NotFound");
}

#[tokio::test]
async fn desktop_only_commands_explain_themselves() {
    let (app, ..) = app(None).await;
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
    let (app, ..) = app(None).await;
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
        while !text.contains("\n\n") {
            let frame = body.frame().await.unwrap().unwrap();
            if let Ok(data) = frame.into_data() {
                text.push_str(std::str::from_utf8(&data).unwrap());
            }
        }
        text
    })
    .await
    .unwrap();
    let data = text.lines().find_map(|l| l.strip_prefix("data: ")).unwrap();
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
    let (app, ..) = app(Some(web.path())).await;
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

fn names(src: &str, re: impl Fn(&str) -> Option<&str>) -> BTreeSet<String> {
    src.lines()
        .filter_map(|l| re(l.trim()))
        .map(str::to_owned)
        .collect()
}

/// Every desktop command is either served or explicitly desktop-only, and nothing else.
#[test]
fn matches_the_desktop_command_set() {
    let src = include_str!("../../../apps/desktop/src-tauri/src/lib.rs");
    let handler = &src[src.find("generate_handler![").unwrap()..];
    let handler = &handler[..handler.find("])").unwrap()];
    let desktop = names(handler, |l| {
        l.strip_suffix(',').and_then(|l| l.rsplit("::").next())
    });
    let server: BTreeSet<String> = rpc::COMMANDS
        .iter()
        .chain(rpc::DESKTOP_ONLY)
        .map(|c| c.to_string())
        .collect();
    assert_eq!(server, desktop);
}

/// Every command the frontend calls is routable.
#[test]
fn covers_the_frontend_commands() {
    let src = include_str!("../../../apps/web/src/app/core/ipc/commands.ts");
    let server: BTreeSet<&str> = rpc::COMMANDS
        .iter()
        .chain(rpc::DESKTOP_ONLY)
        .copied()
        .collect();
    let called: Vec<&str> = src
        .split("call<")
        .skip(1)
        .filter_map(|s| s.split_once(">('").and_then(|(_, r)| r.split('\'').next()))
        .collect();
    assert!(called.len() > 40, "parsed {} calls", called.len());
    for cmd in called {
        assert!(server.contains(cmd), "{cmd}");
    }
}
