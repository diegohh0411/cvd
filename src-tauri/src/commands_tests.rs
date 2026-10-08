use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::engine::{Answer, FakeDecisionEngine, Question};
use crate::render::fit::Selection;
use crate::store::types::{EntryInput, EntryKind, RunStatus};
use crate::store::Store;

fn fake() -> FakeDecisionEngine {
    FakeDecisionEngine::new(|_state, _name, q| match q {
        Question::Noul { .. } => Answer::Noul { p: 0.8 },
        Question::Choice { options, .. } => Answer::Choice {
            choice: options[0].clone(),
            probabilities: options
                .iter()
                .enumerate()
                .map(|(i, o)| (o.clone(), if i == 0 { 0.9 } else { 0.01 }))
                .collect(),
        },
        Question::Score { .. } => Answer::Score {
            score: 3.0,
            probabilities: BTreeMap::from([("3".into(), 1.0)]),
        },
    })
}

type Events = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

async fn state() -> (AppState, Events) {
    let store = Store::open("sqlite::memory:").await.unwrap();
    let events: Events = Arc::default();
    let sink = events.clone();
    let st = AppState::new(
        store,
        std::env::temp_dir().join("cvd-test-models"),
        EngineSource::Fixed(Arc::new(fake())),
        Arc::new(move |name: &str, v: serde_json::Value| {
            sink.lock().unwrap().push((name.to_string(), v))
        }),
    );
    (st, events)
}

fn entry(title: &str) -> EntryInput {
    EntryInput {
        id: None,
        kind: EntryKind::Experience,
        title: title.into(),
        org: "Acme".into(),
        location: "Remote".into(),
        start_date: "2022-01".into(),
        end_date: None,
        description: String::new(),
        sort_hint: 0,
        archived: false,
        bullets: vec!["Built a Rust service".into(), "Shipped SQL migrations".into()],
        skills: vec!["Rust".into()],
    }
}

async fn wait_done(st: &AppState, run_id: i64) -> RunStatus {
    for _ in 0..500 {
        let s = st.store.get_run(run_id).await.unwrap().status;
        if s != RunStatus::Running && !st.is_running(run_id) {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("run {run_id} did not finish");
}

#[tokio::test]
async fn start_match_without_model_errors() {
    let (st, _) = state().await;
    save_entry(&st, entry("Engineer")).await.unwrap();
    let jd = create_job(&st, "Dev", "Co", "Rust developer", "", None).await.unwrap();
    let err = start_match(&st, jd).await.unwrap_err();
    assert_eq!(err.kind(), "ModelMissing");
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["kind"], "ModelMissing");
    assert!(json["message"].is_string());
}

#[tokio::test]
async fn empty_library_rejected() {
    let (st, _) = state().await;
    st.store.set_setting(ACTIVE_MODEL_KEY, "clef-flash").await.unwrap();
    let jd = create_job(&st, "Dev", "Co", "Rust developer", "", None).await.unwrap();
    let err = start_match(&st, jd).await.unwrap_err();
    assert_eq!(
        err,
        AppError::NotFound("Add at least one entry to your library first.".into())
    );
}

#[tokio::test]
async fn match_then_fit_then_export() {
    let (st, events) = state().await;
    st.store.set_setting(ACTIVE_MODEL_KEY, "clef-flash").await.unwrap();
    save_entry(&st, entry("Engineer")).await.unwrap();
    save_entry(&st, entry("Developer")).await.unwrap();
    let jd = create_job(&st, "Dev", "Co", "Rust developer", "", None).await.unwrap();

    let run_id = start_match(&st, jd).await.unwrap();
    assert_eq!(wait_done(&st, run_id).await, RunStatus::Done);
    let evs: Vec<_> = events.lock().unwrap().iter().filter(|(n, _)| n == MATCH_EVENT).cloned().collect();
    assert!(evs.iter().all(|(_, v)| v["run_id"] == run_id));
    assert_eq!(evs.last().unwrap().1["event"]["stage"], "done");

    let ranked = get_ranked(&st, run_id).await.unwrap();
    assert_eq!(ranked.entries.len(), 2);

    let fitted = fit_cv(&st, run_id, 1, vec![]).await.unwrap();
    assert_eq!(fitted.pages, 1);
    assert!(!fitted.overflow);
    assert!(!fitted.pdf_base64.is_empty());
    assert!(!fitted.selection.entries.is_empty());

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cv.pdf");
    let cv_id = export_cv(&st, run_id, fitted.selection.clone(), path.to_string_lossy().into())
        .await
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"%PDF"));

    let cvs = st.store.list_cvs(jd).await.unwrap();
    assert_eq!(cvs.len(), 1);
    assert_eq!(cvs[0].id, cv_id);
    let back: Selection = serde_json::from_str(&cvs[0].selection_json).unwrap();
    assert_eq!(back, fitted.selection);
}

#[tokio::test]
async fn cancel_unknown_run_is_not_found() {
    let (st, _) = state().await;
    assert_eq!(cancel_match(&st, 42).unwrap_err().kind(), "NotFound");
}

#[tokio::test]
async fn shutdown_without_engine_is_noop() {
    let (st, _) = state().await;
    st.shutdown();
}

#[tokio::test]
async fn export_rejects_unsafe_paths() {
    let (st, _) = state().await;
    let sel = Selection { entries: vec![], skills: vec![] };
    for p in ["cv.pdf", "/tmp/cv.txt", "/tmp/cv", "../cv.pdf"] {
        let e = export_cv(&st, 1, sel.clone(), p.into()).await.unwrap_err();
        assert_eq!(e.kind(), "Io", "{p}");
    }
    assert!(validate_export_path("/tmp/CV.PDF").is_ok());
}

#[tokio::test]
async fn fit_rejects_budget_outside_one_two() {
    let (st, _) = state().await;
    for b in [0u8, 3, 255] {
        let e = fit_cv(&st, 1, b, vec![]).await.unwrap_err();
        assert_eq!(e.kind(), "Render");
        assert!(e.to_string().contains("1 or 2"));
    }
}

#[tokio::test]
async fn runs_scores_and_cvs_are_listed_per_job() {
    let (st, events) = state().await;
    st.store.set_setting(ACTIVE_MODEL_KEY, "clef-flash").await.unwrap();
    save_entry(&st, entry("Engineer")).await.unwrap();
    let jd = create_job(&st, "Dev", "Co", "Rust developer", "", None).await.unwrap();
    assert!(list_runs(&st, jd).await.unwrap().is_empty());
    let run = start_match(&st, jd).await.unwrap();
    assert_eq!(wait_done(&st, run).await, RunStatus::Done);

    let runs = list_runs(&st, jd).await.unwrap();
    assert_eq!(runs.iter().map(|r| r.id).collect::<Vec<_>>(), vec![run]);

    let scores = get_scores(&st, run).await.unwrap();
    assert!(scores.iter().any(|s| s.question == "seniority"));
    assert!(scores.iter().any(|s| s.question == "role_family"));
    assert!(scores.iter().all(|s| s.run_id == run));
    assert_eq!(get_scores(&st, 999).await.unwrap_err().kind(), "NotFound");

    assert!(list_cvs(&st, jd).await.unwrap().is_empty());
    assert!(events.lock().unwrap().iter().any(|(n, _)| n == ENGINE_EVENT));
}

#[tokio::test]
async fn engine_status_reflects_active_model_and_errors() {
    let (st, _) = state().await;
    assert_eq!(engine_status(&st).await.unwrap().state, "none");
    st.store.set_setting(ACTIVE_MODEL_KEY, "clef-flash").await.unwrap();
    let s = engine_status(&st).await.unwrap();
    assert_eq!((s.model_id.as_deref(), s.state), (Some("clef-flash"), "idle"));
    st.engine_for("clef-flash").unwrap();
    assert_eq!(engine_status(&st).await.unwrap().state, "ready");
    crate::state::set_engine_error(&st.engine_error_slot(), Some("boom".into()));
    let s = engine_status(&st).await.unwrap();
    assert_eq!((s.state, s.error.as_deref()), ("error", Some("boom")));
    st.reset_engine();
    assert_eq!(engine_status(&st).await.unwrap().state, "idle");
}

#[tokio::test]
async fn ui_settings_are_namespaced() {
    let (st, _) = state().await;
    assert_eq!(get_ui_setting(&st, "ui.page_budget").await.unwrap(), None);
    set_ui_setting(&st, "ui.page_budget", "2").await.unwrap();
    assert_eq!(get_ui_setting(&st, "ui.page_budget").await.unwrap().as_deref(), Some("2"));
    assert_eq!(set_ui_setting(&st, ACTIVE_MODEL_KEY, "x").await.unwrap_err().kind(), "NotFound");
    assert_eq!(get_ui_setting(&st, ACTIVE_MODEL_KEY).await.unwrap_err().kind(), "NotFound");
}

#[tokio::test]
async fn cancel_download_unknown_is_not_found() {
    let (st, _) = state().await;
    assert_eq!(cancel_download(&st, "clef-flash").unwrap_err().kind(), "NotFound");
    let token = st.register_download("clef-flash").unwrap();
    assert!(st.register_download("clef-flash").is_none());
    cancel_download(&st, "clef-flash").unwrap();
    assert!(token.is_cancelled());
    st.finish_download("clef-flash");
    assert!(st.register_download("clef-flash").is_some());
}
