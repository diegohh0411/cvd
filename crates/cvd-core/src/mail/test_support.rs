//! Fakes and fixtures shared by the mail pipeline tests.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::NaiveDate;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::imap::{MailError, MailHeader, MailSource};
use super::scan::{begin_run, run_scan, ScanDeps, ScanSummary};
use crate::engine::{Answer, FakeDecisionEngine};
use crate::providers::ChatEndpoint;
use crate::services::AppError;
use crate::state::Emitter;
use crate::store::{ScanSource, Store};

pub(crate) struct FakeMail {
    pub headers: Vec<MailHeader>,
    pub bodies: HashMap<u32, String>,
    pub check_error: Option<String>,
    pub fetched: Mutex<Vec<u32>>,
}

impl FakeMail {
    pub(crate) fn new(headers: Vec<MailHeader>) -> Self {
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

    pub(crate) fn fetched(&self) -> Vec<u32> {
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

    async fn body_by_message_id(&self, message_id: &str) -> Result<Option<String>, MailError> {
        let Some(uid) = self
            .headers
            .iter()
            .find(|h| h.message_id == message_id)
            .map(|h| h.uid)
        else {
            return Ok(None);
        };
        self.body_text(uid).await.map(Some)
    }
}

pub(crate) fn header(uid: u32, message_id: &str) -> MailHeader {
    MailHeader {
        uid,
        message_id: message_id.to_string(),
        subject: format!("Subject {uid}"),
        sender: "recruiter@acme.com".into(),
        received_at: "2026-10-10T09:00:00+00:00".into(),
    }
}

pub(crate) fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 12).unwrap()
}

pub(crate) fn probs(pairs: &[(&str, f64)]) -> std::collections::BTreeMap<String, f64> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

/// Seeded statuses: Bookmarked, Applied, Interviewing, Offer. One job at Applied.
pub(crate) async fn seeded() -> (Store, i64, i64, i64) {
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
pub(crate) fn scripted(
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

pub(crate) fn ignoring() -> FakeDecisionEngine {
    scripted(0.1, "no_match".into(), 0.1, "status_1".into(), 0.5)
}

/// Chat endpoint that answers every summary request with one sentence.
pub(crate) async fn chat_endpoint(server: &MockServer) -> ChatEndpoint {
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
pub(crate) fn unreachable_chat() -> ChatEndpoint {
    ChatEndpoint {
        base_url: "http://127.0.0.1:9/v1".into(),
        api_key: None,
        model: "chat-model".into(),
    }
}

pub(crate) async fn run(
    store: &Store,
    mail: &FakeMail,
    engine: &FakeDecisionEngine,
    chat: &ChatEndpoint,
) -> Result<ScanSummary, AppError> {
    let emit: Emitter = Arc::new(|_: &str, _: serde_json::Value| {});
    run_with(store, mail, engine, chat, ScanSource::Manual, &emit).await
}

pub(crate) async fn run_with(
    store: &Store,
    mail: &FakeMail,
    engine: &FakeDecisionEngine,
    chat: &ChatEndpoint,
    source: ScanSource,
    emit: &Emitter,
) -> Result<ScanSummary, AppError> {
    let http = reqwest::Client::new();
    let run_id = begin_run(store, source).await?;
    run_scan(
        ScanDeps {
            store,
            mail,
            engine,
            chat,
            http: &http,
            emit,
            today: today(),
        },
        run_id,
        source,
    )
    .await
}

pub(crate) type Events = Arc<Mutex<Vec<(String, serde_json::Value)>>>;

pub(crate) fn capturing() -> (Emitter, Events) {
    let events: Events = Arc::default();
    let sink = events.clone();
    let emit: Emitter = Arc::new(move |name: &str, payload: serde_json::Value| {
        sink.lock().unwrap().push((name.to_string(), payload));
    });
    (emit, events)
}
