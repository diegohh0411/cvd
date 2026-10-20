use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::NaiveDate;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::imap::{MailError, MailHeader, MailSource};
use super::scan::{begin_run, run_scan, ScanDeps, ScanSummary};
use crate::commands::AppError;
use crate::engine::{Answer, FakeDecisionEngine};
use crate::providers::ChatEndpoint;
use crate::state::Emitter;
use crate::store::{NewProcessed, Outcome, ScanSource, ScanStatus, Store};

struct FakeMail {
    headers: Vec<MailHeader>,
    bodies: HashMap<u32, String>,
    check_error: Option<String>,
    fetched: Mutex<Vec<u32>>,
}

impl FakeMail {
    fn new(headers: Vec<MailHeader>) -> Self {
        let bodies = headers
            .iter()
            .map(|h| (h.uid, format!("body of {}", h.uid)))
            .collect();
        Self {
            headers,
            bodies,
            check_error: None,
            fetched: Mutex::default(),
        }
    }

    fn fetched(&self) -> Vec<u32> {
        self.fetched.lock().unwrap().clone()
    }
}

#[async_trait]
impl MailSource for FakeMail {
    async fn check(&self) -> Result<(), MailError> {
        match &self.check_error {
            Some(message) => Err(MailError::Connection(message.clone())),
            None => Ok(()),
        }
    }

    async fn recent_headers(&self, _since: NaiveDate) -> Result<Vec<MailHeader>, MailError> {
        Ok(self.headers.clone())
    }

    async fn body_text(&self, uid: u32) -> Result<String, MailError> {
        self.fetched.lock().unwrap().push(uid);
        self.bodies
            .get(&uid)
            .cloned()
            .ok_or_else(|| MailError::Protocol(format!("no body for {uid}")))
    }
}

fn header(uid: u32, message_id: &str) -> MailHeader {
    MailHeader {
        uid,
        message_id: message_id.to_string(),
        subject: format!("Subject {uid}"),
        sender: "recruiter@acme.com".into(),
        received_at: "2026-10-10T09:00:00+00:00".into(),
    }
}

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 12).unwrap()
}

fn probs(pairs: &[(&str, f64)]) -> std::collections::BTreeMap<String, f64> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

/// Seeded statuses: Bookmarked, Applied, Interviewing, Offer. One job at Applied.
async fn seeded() -> (Store, i64, i64, i64) {
    let store = Store::open("sqlite::memory:").await.unwrap();
    let applied = store.find_job_status("Applied").await.unwrap().unwrap().id;
    let interviewing = store
        .find_job_status("Interviewing")
        .await
        .unwrap()
        .unwrap()
        .id;
    let job = store
        .create_jd("Engineer", "Acme", "raw", "", Some(applied))
        .await
        .unwrap();
    (store, job, applied, interviewing)
}

/// Every question answered from fixed values. `job` and `status` are option keys.
fn scripted(
    job_related: f64,
    job: String,
    status_changed: f64,
    status: String,
    confidence: f64,
) -> FakeDecisionEngine {
    FakeDecisionEngine::new(move |_, key, _| match key {
        "job_related" => Answer::Noul { p: job_related },
        "matched_job" => Answer::Choice {
            choice: job.clone(),
            probabilities: probs(&[(job.as_str(), 1.0)]),
        },
        "status_changed" => Answer::Noul { p: status_changed },
        "new_status" => Answer::Choice {
            choice: status.clone(),
            probabilities: probs(&[(status.as_str(), confidence)]),
        },
        other => panic!("unexpected question {other}"),
    })
}

fn ignoring() -> FakeDecisionEngine {
    scripted(0.1, "no_match".into(), 0.1, "status_1".into(), 0.5)
}

/// Chat endpoint that answers every summary request with one sentence.
async fn chat_endpoint(server: &MockServer) -> ChatEndpoint {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{ "message": { "role": "assistant", "content": "Your application moved forward." } }]
        })))
        .mount(server)
        .await;
    ChatEndpoint {
        base_url: format!("{}/v1", server.uri()),
        api_key: None,
        model: "chat-model".into(),
    }
}

/// A chat endpoint that refuses connections; any summary call fails loudly.
fn unreachable_chat() -> ChatEndpoint {
    ChatEndpoint {
        base_url: "http://127.0.0.1:9/v1".into(),
        api_key: None,
        model: "chat-model".into(),
    }
}

async fn run(
    store: &Store,
    mail: &FakeMail,
    engine: &FakeDecisionEngine,
    chat: &ChatEndpoint,
) -> Result<ScanSummary, AppError> {
    let emit: Emitter = Arc::new(|_: &str, _: serde_json::Value| {});
    let http = reqwest::Client::new();
    let run_id = begin_run(store).await?;
    run_scan(
        ScanDeps {
            store,
            mail,
            engine,
            chat,
            http: &http,
            emit: &emit,
            today: today(),
        },
        run_id,
    )
    .await
}

#[tokio::test]
async fn skips_processed_messages_without_fetching_bodies() {
    let (store, _, _, _) = seeded().await;
    let run_id = store.create_scan_run(ScanSource::Manual).await.unwrap();
    store
        .record_processed(
            &NewProcessed {
                message_id: "<a>".into(),
                run_id,
                received_at: "2026-10-01T00:00:00+00:00".into(),
                subject: "old".into(),
                sender: "x@example.com".into(),
                matched_job_id: None,
                previous_status_id: None,
                suggested_status_id: None,
                confidence: None,
                summary: None,
                outcome: Outcome::Ignored,
                job_related: None,
                body: None,
            },
            None,
        )
        .await
        .unwrap();
    let mail = FakeMail::new(vec![header(1, "<a>"), header(2, "<b>")]);

    run(&store, &mail, &ignoring(), &unreachable_chat())
        .await
        .unwrap();

    assert_eq!(mail.fetched(), vec![2]);
}

#[tokio::test]
async fn ignored_email_is_recorded_without_chat_call() {
    let (store, _, _, _) = seeded().await;
    let mail = FakeMail::new(vec![header(1, "<a>")]);

    let summary = run(&store, &mail, &ignoring(), &unreachable_chat())
        .await
        .unwrap();

    assert_eq!(summary.checked, 1);
    assert_eq!(summary.job_related, 0);
    let row = store.get_processed("<a>").await.unwrap().unwrap();
    assert_eq!(row.outcome, Outcome::Ignored);
    assert_eq!(row.summary, None);
}

#[tokio::test]
async fn applied_email_updates_job_status() {
    let server = MockServer::start().await;
    let chat = chat_endpoint(&server).await;
    let (store, job, applied, interviewing) = seeded().await;
    let mail = FakeMail::new(vec![header(1, "<a>")]);
    let engine = scripted(
        0.95,
        format!("job_{job}"),
        0.95,
        format!("status_{interviewing}"),
        0.9,
    );

    let summary = run(&store, &mail, &engine, &chat).await.unwrap();

    assert_eq!(summary.applied, 1);
    assert_eq!(summary.failed, 0);
    assert_eq!(
        store.get_jd(job).await.unwrap().unwrap().status_id,
        Some(interviewing)
    );
    let row = store.get_processed("<a>").await.unwrap().unwrap();
    assert_eq!(row.outcome, Outcome::Applied);
    assert_eq!(row.previous_status_id, Some(applied));
    assert_eq!(row.suggested_status_id, Some(interviewing));
    assert_eq!(row.matched_job_id, Some(job));
}

#[tokio::test]
async fn pending_email_leaves_job_status_unchanged() {
    let server = MockServer::start().await;
    let chat = chat_endpoint(&server).await;
    let (store, job, applied, interviewing) = seeded().await;
    let mail = FakeMail::new(vec![header(1, "<a>")]);
    let engine = scripted(
        0.95,
        format!("job_{job}"),
        0.6,
        format!("status_{interviewing}"),
        0.9,
    );

    let summary = run(&store, &mail, &engine, &chat).await.unwrap();

    assert_eq!(summary.pending, 1);
    assert_eq!(
        store.get_jd(job).await.unwrap().unwrap().status_id,
        Some(applied)
    );
    let row = store.get_processed("<a>").await.unwrap().unwrap();
    assert_eq!(row.outcome, Outcome::PendingReview);
    assert_eq!(row.suggested_status_id, Some(interviewing));
    assert_eq!(row.confidence, Some(0.9));
}

#[tokio::test]
async fn no_change_email_gets_summary_and_outcome_no_change() {
    let server = MockServer::start().await;
    let chat = chat_endpoint(&server).await;
    let (store, job, applied, _) = seeded().await;
    let mail = FakeMail::new(vec![header(1, "<a>")]);
    let engine = scripted(
        0.95,
        format!("job_{job}"),
        0.9,
        format!("status_{applied}"),
        0.95,
    );

    let summary = run(&store, &mail, &engine, &chat).await.unwrap();

    assert_eq!(summary.no_change, 1);
    let row = store.get_processed("<a>").await.unwrap().unwrap();
    assert_eq!(row.outcome, Outcome::NoChange);
    assert_eq!(
        row.summary.as_deref(),
        Some("Your application moved forward.")
    );
}

#[tokio::test]
async fn bad_engine_response_is_retried_next_run() {
    let (store, job, _, interviewing) = seeded().await;
    let mail = FakeMail::new(vec![header(1, "<a>")]);
    // Unknown job option, so classification returns BadResponse.
    let bad = scripted(
        0.95,
        "job_999".into(),
        0.95,
        format!("status_{interviewing}"),
        0.9,
    );

    let first = run(&store, &mail, &bad, &unreachable_chat()).await.unwrap();

    assert_eq!(first.failed, 1);
    assert!(store.get_processed("<a>").await.unwrap().is_none());

    let server = MockServer::start().await;
    let good = scripted(
        0.95,
        format!("job_{job}"),
        0.95,
        format!("status_{interviewing}"),
        0.9,
    );
    let second = run(&store, &mail, &good, &chat_endpoint(&server).await)
        .await
        .unwrap();

    assert_eq!(second.applied, 1);
    assert!(store.get_processed("<a>").await.unwrap().is_some());
}

#[tokio::test]
async fn fatal_engine_error_fails_the_run() {
    let (store, _, _, _) = seeded().await;
    let mail = FakeMail::new(vec![header(1, "<a>"), header(2, "<b>"), header(3, "<c>")]);
    let engine = ignoring();
    // Emails run concurrently, so which one hits call 5 is not fixed; the run must fail either way.
    engine.fail_on_call(5);

    let result = run(&store, &mail, &engine, &unreachable_chat()).await;

    assert!(
        matches!(result, Err(AppError::EngineUnavailable(_))),
        "{result:?}"
    );
    let run = store.get_scan_run(1).await.unwrap().unwrap();
    assert_eq!(run.status, ScanStatus::Failed);
    assert!(run.error.is_some());
}

#[tokio::test]
async fn imap_check_failure_fails_run() {
    let (store, _, _, _) = seeded().await;
    let mut mail = FakeMail::new(vec![header(1, "<a>")]);
    mail.check_error = Some("connection refused".into());

    let result = run(&store, &mail, &ignoring(), &unreachable_chat()).await;

    assert!(
        matches!(result, Err(AppError::MailUnavailable(_))),
        "{result:?}"
    );
    let run = store.get_scan_run(1).await.unwrap().unwrap();
    assert_eq!(run.status, ScanStatus::Failed);
}

#[tokio::test]
async fn zero_statuses_refuses_to_start() {
    let (store, _, _, _) = seeded().await;
    for status in store.list_job_statuses().await.unwrap() {
        store.delete_job_status(status.id).await.unwrap();
    }
    let mail = FakeMail::new(vec![header(1, "<a>")]);

    let result = run(&store, &mail, &ignoring(), &unreachable_chat()).await;

    assert!(matches!(result, Err(AppError::Io(_))), "{result:?}");
    assert!(store.get_scan_run(1).await.unwrap().is_none());
}
