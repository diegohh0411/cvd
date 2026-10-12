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

#[tokio::test]
async fn missing_key_status() {
    let (st, _) = state().await;
    st.reload_providers().await.unwrap();
    st.store.set_setting(ACTIVE_MODEL_KEY, "openrouter:typesafe/jev-1.13").await.unwrap();
    let s = engine_status(&st).await.unwrap();
    assert_eq!(s.state, "error");
    assert!(s.error.unwrap().contains("OpenRouter key"));
    st.store.upsert_entry(&entry("Engineer")).await.unwrap();
    let jd = st.store.create_jd("Rust role", "Acme", "We need Rust", "", None).await.unwrap();
    let err = start_match(&st, jd).await.unwrap_err();
    assert!(err.to_string().contains("OpenRouter key"), "{err}");
    assert!(st.engine_for("openrouter:typesafe/jev-1.13").is_err());
}

#[tokio::test]
async fn bare_local_id_still_local() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    let st = AppState::new(
        store,
        std::env::temp_dir().join("cvd-test-models-empty"),
        EngineSource::Fixed(Arc::new(fake())),
        Arc::new(|_: &str, _| {}),
    );
    st.reload_providers().await.unwrap();
    // Local refs keep the Fixed short-circuit; remote refs never do.
    assert!(st.engine_for("clef-flash").is_ok());
    assert!(st.engine_for("openrouter:x").is_err());
    assert_eq!(st.installed_path("clef-flash").unwrap_err().kind(), "ModelMissing");
}

mod provider_keys {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn keyed_state(server: &MockServer) -> AppState {
        let (st, _) = state().await;
        st.with_openrouter_base(server.uri())
    }

    async fn key_server(valid: &str) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/key"))
            .and(header("authorization", format!("Bearer {valid}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": {}})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/key"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({"error": {"message": "User not found."}})))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn set_key_trims_and_validates() {
        let server = key_server("sk-or-abcd1234").await;
        let st = keyed_state(&server).await;
        let status = set_provider_key(&st, "openrouter", "  sk-or-abcd1234 \n").await.unwrap();
        assert_eq!(status, KeyStatus { has_key: true, last4: Some("1234".into()) });
        assert_eq!(st.store.get_secret("openrouter").await.unwrap().as_deref(), Some("sk-or-abcd1234"));
        assert_eq!(provider_key_status(&st, "openrouter").await.unwrap(), status);
    }

    #[tokio::test]
    async fn set_key_rejects_invalid() {
        let server = key_server("good").await;
        let st = keyed_state(&server).await;
        let err = set_provider_key(&st, "openrouter", "bad").await.unwrap_err();
        assert_eq!(err.kind(), "ProviderAuth");
        assert_eq!(st.store.get_secret("openrouter").await.unwrap(), None);
        assert_eq!(set_provider_key(&st, "nope", "x").await.unwrap_err().kind(), "NotFound");
    }

    #[tokio::test]
    async fn delete_key_unregisters() {
        let server = key_server("good").await;
        let st = keyed_state(&server).await;
        set_provider_key(&st, "openrouter", "good").await.unwrap();
        assert!(st.engine_for("openrouter:typesafe/jev-1.13").is_ok());
        delete_provider_key(&st, "openrouter").await.unwrap();
        assert!(st.engine_for("openrouter:typesafe/jev-1.13").is_err());
        assert_eq!(provider_key_status(&st, "openrouter").await.unwrap(), KeyStatus { has_key: false, last4: None });
    }

    #[tokio::test]
    async fn set_active_remote_requires_key() {
        let server = key_server("good").await;
        let st = keyed_state(&server).await;
        assert_eq!(set_active_model(&st, "openrouter:typesafe/jev-1.13").await.unwrap_err().kind(), "ModelMissing");
        set_provider_key(&st, "openrouter", "good").await.unwrap();
        set_active_model(&st, "openrouter:typesafe/jev-1.13").await.unwrap();
        assert_eq!(st.active_model().await.unwrap(), "openrouter:typesafe/jev-1.13");
    }

    async fn exa_server(valid: &str) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/search"))
            .and(header("x-api-key", valid))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"results": []})))
            .mount(&server)
            .await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(401)).mount(&server).await;
        server
    }

    #[tokio::test]
    async fn exa_key_validates_and_enables_search() {
        let server = exa_server("exa-good").await;
        let (st, _) = state().await;
        let st = st.with_exa_base(server.uri());
        assert_eq!(st.search().err().map(|e| e.to_string()).as_deref(), Some("Add your Exa key in Settings."));
        assert_eq!(set_provider_key(&st, "exa", "bad").await.unwrap_err().kind(), "ProviderAuth");
        assert_eq!(st.store.get_secret("exa").await.unwrap(), None);
        let status = set_provider_key(&st, "exa", " exa-good ").await.unwrap();
        assert_eq!(status.last4.as_deref(), Some("good"));
        assert!(st.search().is_ok());
        delete_provider_key(&st, "exa").await.unwrap();
        assert!(st.search().is_err());
    }

    #[tokio::test]
    async fn research_model_role() {
        let server = key_server("good").await;
        let st = keyed_state(&server).await;
        let err = st.research_endpoint().await.unwrap_err();
        assert_eq!(err.kind(), "ModelMissing");
        assert_eq!(err.to_string(), "Choose a Company research model in Settings.");
        assert_eq!(set_research_model(&st, "clef-flash").await.unwrap_err().kind(), "NotFound");
        assert_eq!(set_active_model(&st, "lfm2.5-2.6b").await.unwrap_err().kind(), "NotFound");
        assert_eq!(set_research_model(&st, "openrouter:deepseek/x").await.unwrap_err().kind(), "ModelMissing");
        set_provider_key(&st, "openrouter", "good").await.unwrap();
        set_research_model(&st, "openrouter:deepseek/x").await.unwrap();
        let ep = st.research_endpoint().await.unwrap();
        assert_eq!(ep.model, "deepseek/x");
        assert_eq!(ep.base_url, format!("{}/v1", server.uri()));
        assert_eq!(st.research_model().await.unwrap().as_deref(), Some("openrouter:deepseek/x"));
    }

    #[test]
    fn engine_errors_map_to_provider_kinds() {
        assert_eq!(AppError::from(EngineError::Unauthorized).kind(), "ProviderAuth");
        assert_eq!(AppError::from(EngineError::InsufficientCredits).kind(), "ProviderCredits");
        assert_eq!(AppError::from(EngineError::RateLimited).kind(), "ProviderUnavailable");
        assert_eq!(AppError::from(EngineError::Network("x".into())).kind(), "ProviderUnavailable");
        assert_eq!(AppError::from(EngineError::ModelUnavailable("m".into())).kind(), "ProviderUnavailable");
    }
}

mod company_flow {
    use super::*;
    use crate::company::agent::{ResearchError, ResearchOutput, ResearchStep};
    use crate::company::coordinator::ResearchFn;
    use crate::engine::DecisionEngine;
    use crate::store::types::{CompanyProfile, CompanyStatus};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio_util::sync::CancellationToken;

    /// Records the System One state of every request.
    struct StateLog(FakeDecisionEngine, Mutex<Vec<String>>);

    #[async_trait::async_trait]
    impl DecisionEngine for StateLog {
        async fn decide(
            &self,
            state: &str,
            q: &BTreeMap<String, Question>,
        ) -> Result<BTreeMap<String, Answer>, EngineError> {
            self.1.lock().unwrap().push(state.to_string());
            self.0.decide(state, q).await
        }
    }

    fn research(calls: Arc<AtomicUsize>, delay: Duration, result: Result<ResearchOutput, ResearchError>) -> ResearchFn {
        Arc::new(move |_i, cancel: CancellationToken, on_step| {
            calls.fetch_add(1, Ordering::SeqCst);
            let result = result.clone();
            Box::pin(async move {
                on_step(ResearchStep::Round { n: 1, max: 25 });
                tokio::select! {
                    _ = cancel.cancelled() => Err(ResearchError::Cancelled),
                    _ = tokio::time::sleep(delay) => result.map(|o| (o, "lfm".to_string())),
                }
            })
        })
    }

    fn acme() -> ResearchOutput {
        ResearchOutput {
            canonical_name: "Acme".into(),
            domain: Some("acme.com".into()),
            profile: CompanyProfile { industry: "Fintech".into(), ..Default::default() },
        }
    }

    async fn setup(f: ResearchFn) -> (AppState, Arc<StateLog>, Events) {
        let store = Store::open("sqlite::memory:").await.unwrap();
        let events: Events = Arc::default();
        let sink = events.clone();
        let log = Arc::new(StateLog(fake(), Mutex::default()));
        let st = AppState::new(
            store,
            std::env::temp_dir().join("cvd-test-models"),
            EngineSource::Fixed(log.clone()),
            Arc::new(move |n: &str, v: serde_json::Value| sink.lock().unwrap().push((n.to_string(), v))),
        )
        .with_research_fn(f);
        st.store.set_setting(ACTIVE_MODEL_KEY, "clef-flash").await.unwrap();
        save_entry(&st, entry("Engineer")).await.unwrap();
        (st, log, events)
    }

    fn researching(events: &Events, run: i64) -> bool {
        events.lock().unwrap().iter().any(|(n, v)| n == MATCH_EVENT && v["run_id"] == run && v["event"]["stage"] == "researching")
    }

    #[tokio::test]
    async fn empty_company_scores_without_research() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (st, log, events) = setup(research(calls.clone(), Duration::ZERO, Ok(acme()))).await;
        let jd = create_job(&st, "Dev", "", "Rust developer", "", None).await.unwrap();
        let run = start_match(&st, jd).await.unwrap();
        assert_eq!(wait_done(&st, run).await, RunStatus::Done);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert_eq!(st.store.get_run(run).await.unwrap().company_researched_at, None);
        assert!(!researching(&events, run));
        assert!(log.1.lock().unwrap().iter().all(|s| s.starts_with("JOB DESCRIPTION:")));
    }

    #[tokio::test]
    async fn researched_profile_reaches_state() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (st, log, events) = setup(research(calls.clone(), Duration::ZERO, Ok(acme()))).await;
        let jd = create_job(&st, "Dev", "Acme Inc.", "Rust developer", "https://www.linkedin.com/jobs/1", None).await.unwrap();
        let run = start_match(&st, jd).await.unwrap();
        assert_eq!(wait_done(&st, run).await, RunStatus::Done);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(researching(&events, run));
        let states = log.1.lock().unwrap().clone();
        assert!(states.iter().all(|s| s.starts_with(&format!("{}Name: Acme (acme.com)\nIndustry: Fintech", crate::matcher::questions::PROFILE_HEADER))), "{states:?}");
        let company = get_job_company(&st, jd).await.unwrap().unwrap();
        assert_eq!(company.status, CompanyStatus::Ok);
        assert_eq!(st.store.get_run(run).await.unwrap().company_researched_at, company.researched_at);
        // A second match reuses the fresh profile.
        let run2 = start_match(&st, jd).await.unwrap();
        wait_done(&st, run2).await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    async fn wait_researching(events: &Events, run: i64) {
        for _ in 0..200 {
            if researching(events, run) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("run {run} never started researching");
    }

    #[tokio::test]
    async fn skip_background_scores_and_research_finishes() {
        let (st, log, events) = setup(research(Arc::default(), Duration::from_millis(300), Ok(acme()))).await;
        let jd = create_job(&st, "Dev", "Acme", "Rust developer", "", None).await.unwrap();
        let run = start_match(&st, jd).await.unwrap();
        wait_researching(&events, run).await;
        skip_research(&st, run, "background").await.unwrap();
        assert_eq!(wait_done(&st, run).await, RunStatus::Done);
        assert_eq!(st.store.get_run(run).await.unwrap().company_researched_at, None);
        assert!(log.1.lock().unwrap().iter().all(|s| s.starts_with("JOB DESCRIPTION:")));
        for _ in 0..100 {
            if get_job_company(&st, jd).await.unwrap().unwrap().status == CompanyStatus::Ok {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("background research did not finish");
    }

    #[tokio::test]
    async fn skip_cancel_scores_and_research_cancelled() {
        let (st, _, events) = setup(research(Arc::default(), Duration::from_secs(30), Ok(acme()))).await;
        let jd = create_job(&st, "Dev", "Acme", "Rust developer", "", None).await.unwrap();
        let run = start_match(&st, jd).await.unwrap();
        wait_researching(&events, run).await;
        skip_research(&st, run, "cancel").await.unwrap();
        assert_eq!(wait_done(&st, run).await, RunStatus::Done);
        let c = get_job_company(&st, jd).await.unwrap().unwrap();
        assert_eq!(c.status, CompanyStatus::Pending);
        assert_eq!(c.fail_count, 0);
        assert_eq!(skip_research(&st, run, "cancel").await.unwrap_err().kind(), "NotFound");
    }

    #[tokio::test]
    async fn research_failure_scores_without_context() {
        let (st, _, _) = setup(research(Arc::default(), Duration::ZERO, Err(ResearchError::Budget))).await;
        let jd = create_job(&st, "Dev", "Acme", "Rust developer", "", None).await.unwrap();
        let run = start_match(&st, jd).await.unwrap();
        assert_eq!(wait_done(&st, run).await, RunStatus::Done);
        assert_eq!(st.store.get_run(run).await.unwrap().company_researched_at, None);
        assert_eq!(get_job_company(&st, jd).await.unwrap().unwrap().status, CompanyStatus::Failed);
    }

    #[tokio::test]
    async fn no_research_model_scores_without_context() {
        let (st, events) = state().await;
        st.store.set_setting(ACTIVE_MODEL_KEY, "clef-flash").await.unwrap();
        save_entry(&st, entry("Engineer")).await.unwrap();
        let jd = create_job(&st, "Dev", "Acme", "Rust developer", "", None).await.unwrap();
        let run = start_match(&st, jd).await.unwrap();
        assert_eq!(wait_done(&st, run).await, RunStatus::Done);
        let failed = events.lock().unwrap().iter().any(|(n, v)| {
            n == crate::company::coordinator::RESEARCH_EVENT
                && v["event"]["error"] == "Choose a Company research model in Settings."
        });
        assert!(failed);
        assert_eq!(get_job_company(&st, jd).await.unwrap().unwrap().fail_count, 0);
    }

    #[tokio::test]
    async fn rescore_creates_new_run_with_current_profile() {
        let (st, _, events) = setup(research(Arc::default(), Duration::from_secs(30), Ok(acme()))).await;
        let jd = create_job(&st, "Dev", "Acme", "Rust developer", "", None).await.unwrap();
        let run = start_match(&st, jd).await.unwrap();
        wait_researching(&events, run).await;
        skip_research(&st, run, "cancel").await.unwrap();
        wait_done(&st, run).await;
        let c = get_job_company(&st, jd).await.unwrap().unwrap();
        st.store.save_research(c.id, "Acme", Some("acme.com"), &acme().profile, "lfm").await.unwrap();
        let run2 = rescore(&st, run).await.unwrap();
        assert_ne!(run2, run);
        assert_eq!(wait_done(&st, run2).await, RunStatus::Done);
        assert!(st.store.get_run(run2).await.unwrap().company_researched_at.is_some());
    }

    #[tokio::test]
    async fn edit_then_refresh_overwrites() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (st, _, _) = setup(research(calls.clone(), Duration::ZERO, Ok(acme()))).await;
        let jd = create_job(&st, "Dev", "Acme", "Rust developer", "", None).await.unwrap();
        wait_done(&st, start_match(&st, jd).await.unwrap()).await;
        let c = get_job_company(&st, jd).await.unwrap().unwrap();
        let edited = edit_company(&st, c.id, CompanyProfile { industry: "Mine".into(), ..Default::default() }).await.unwrap();
        assert!(edited.edited_at.is_some());
        assert_eq!(get_company(&st, c.id).await.unwrap().profile.industry, "Mine");
        let refreshed = refresh_company(&st, c.id).await.unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(refreshed.profile.industry, "Fintech");
        assert_eq!(refreshed.edited_at, None);
    }

    #[tokio::test]
    async fn save_mail_account_rejects_bad_port() {
        let (st, _) = state().await;

        let err = save_mail_account(&st, "imap.gmail.com", 0, "me@gmail.com", Some("abcd efgh ijkl mnop"))
            .await
            .unwrap_err();

        assert_eq!(err.kind(), "Io");
    }

    #[tokio::test]
    async fn save_mail_account_keeps_password_when_none() {
        let (st, _) = state().await;
        save_mail_account(&st, "imap.gmail.com", 993, "me@gmail.com", Some("abcd efgh ijkl mnop"))
            .await
            .unwrap();

        save_mail_account(&st, "imap.gmail.com", 993, "other@gmail.com", None)
            .await
            .unwrap();

        let stored = crate::mail::account::load(&st.store).await.unwrap().unwrap();
        assert_eq!(stored.address, "other@gmail.com");
        assert_eq!(stored.password, "abcdefghijklmnop");
    }

    #[tokio::test]
    async fn get_mail_account_never_returns_the_password() {
        let (st, _) = state().await;
        save_mail_account(&st, "imap.gmail.com", 993, "me@gmail.com", Some("abcd efgh ijkl mnop"))
            .await
            .unwrap();

        let view = get_mail_account(&st).await.unwrap().unwrap();

        assert!(view.has_password);
        let json = serde_json::to_value(&view).unwrap();
        assert!(json.get("password").is_none());
        assert!(!json.to_string().contains("abcdefghijklmnop"));
    }

    #[tokio::test]
    async fn start_scan_refuses_second_concurrent_start() {
        let (st, _) = state().await;
        let _held = st.begin_scan().unwrap();

        let err = start_scan(&st).await.unwrap_err();

        assert_eq!(err.kind(), "Io");
        assert_eq!(err.to_string(), "A scan is already running.");
    }

    #[tokio::test]
    async fn apply_suggestion_unknown_message_is_not_found() {
        let (st, _) = state().await;

        let err = apply_suggestion(&st, "<missing@example.com>").await.unwrap_err();

        assert_eq!(err.kind(), "NotFound");
    }
}
