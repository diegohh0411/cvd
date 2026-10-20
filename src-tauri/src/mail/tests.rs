use std::collections::BTreeMap;

use async_trait::async_trait;

use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::commands::AppError;
use crate::engine::{Answer, DecisionEngine, EngineError, FakeDecisionEngine, Question};
use crate::providers::ChatEndpoint;
use crate::store::types::{JobStatus, StatusColor};
use crate::store::Store;

use super::account::{self, MailAccount};
use super::classify::{
    build_questions, build_state, classify, job_label, EmailInput, JobCtx, BODY_CHARS,
};
use super::decide::{decide, Classified, Decision};
use super::imap::{chunk_uids, extract_text, gmail_link, parse_header, ImapSource, MailSource};
use super::summary::summarize;

fn classified(
    job_related: f64,
    matched_job: Option<i64>,
    status_changed: f64,
    new_status: Option<(i64, f64)>,
) -> Classified {
    Classified {
        job_related,
        matched_job,
        status_changed,
        new_status,
    }
}

#[test]
fn job_related_exactly_half_counts() {
    assert_eq!(
        decide(&classified(0.5, Some(7), 0.9, Some((3, 0.9))), None),
        Decision::Applied {
            job_id: 7,
            status_id: 3,
            confidence: 0.9,
        }
    );
}

#[test]
fn job_related_just_below_half_is_ignored() {
    assert_eq!(
        decide(&classified(0.499, Some(7), 0.9, Some((3, 0.9))), None),
        Decision::Ignored
    );
}

#[test]
fn applies_at_exact_threshold() {
    assert_eq!(
        decide(&classified(0.9, Some(7), 0.85, Some((3, 0.85))), None),
        Decision::Applied {
            job_id: 7,
            status_id: 3,
            confidence: 0.85,
        }
    );
}

#[test]
fn just_below_threshold_is_pending() {
    assert_eq!(
        decide(&classified(0.9, Some(7), 0.85, Some((3, 0.849))), None),
        Decision::PendingReview {
            job_id: 7,
            status_id: 3,
            confidence: 0.849,
        }
    );
}

#[test]
fn same_status_is_no_change() {
    assert_eq!(
        decide(&classified(0.9, Some(7), 0.9, Some((3, 0.9))), Some(3)),
        Decision::NoChange { job_id: 7 }
    );
}

#[test]
fn status_change_below_half_is_no_change() {
    assert_eq!(
        decide(&classified(0.9, Some(7), 0.499, Some((3, 0.9))), None),
        Decision::NoChange { job_id: 7 }
    );
}

#[test]
fn missing_new_status_is_no_change() {
    assert_eq!(
        decide(&classified(0.9, Some(7), 0.9, None), None),
        Decision::NoChange { job_id: 7 }
    );
}

#[test]
fn missing_match_is_ignored() {
    assert_eq!(
        decide(&classified(0.9, None, 0.9, Some((3, 0.9))), None),
        Decision::Ignored
    );
}

async fn store() -> Store {
    Store::open("sqlite::memory:").await.unwrap()
}

fn account() -> MailAccount {
    MailAccount {
        host: "imap.gmail.com".into(),
        port: 993,
        address: "user@gmail.com".into(),
        password: "abcdefghijklmnop".into(),
    }
}

#[tokio::test]
async fn save_then_load_round_trips() {
    let store = store().await;
    let expected = account();
    account::save(&store, &expected).await.unwrap();
    assert!(account::load(&store).await.unwrap() == Some(expected));
}

#[tokio::test]
async fn password_spaces_are_stripped_on_save() {
    let store = store().await;
    let mut account = account();
    account.password = "abcd efgh ijkl mnop".into();
    account::save(&store, &account).await.unwrap();
    assert_eq!(
        account::load(&store).await.unwrap().unwrap().password,
        "abcdefghijklmnop"
    );
}

#[tokio::test]
async fn load_is_none_when_incomplete() {
    let store = store().await;
    store
        .set_setting("mail.address", "user@gmail.com")
        .await
        .unwrap();
    assert!(account::load(&store).await.unwrap().is_none());
}

#[tokio::test]
async fn remove_clears_everything() {
    let store = store().await;
    account::save(&store, &account()).await.unwrap();
    account::remove(&store).await.unwrap();
    assert!(account::load(&store).await.unwrap().is_none());
    assert_eq!(store.get_setting("mail.host").await.unwrap(), None);
    assert_eq!(store.get_setting("mail.port").await.unwrap(), None);
    assert_eq!(store.get_setting("mail.address").await.unwrap(), None);
    assert_eq!(store.get_secret("gmail").await.unwrap(), None);
}

#[test]
fn validate_rejects_bad_fields() {
    assert!(account::validate("", 993, "user@gmail.com").is_err());
    assert!(account::validate("imap.gmail.com", 0, "user@gmail.com").is_err());
    assert!(account::validate("imap.gmail.com", 993, "invalid-address").is_err());
}

#[test]
fn html_only_email_yields_readable_text() {
    let raw = b"From: Recruiter <recruiter@example.com>\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<html><body><h1>Interview invitation</h1><p>Please choose a time.</p></body></html>";
    let text = extract_text(raw);
    assert!(text.contains("Interview invitation"));
    assert!(text.contains("Please choose a time."));
    assert!(!text.contains("<h1>"));
}

#[test]
fn plain_part_preferred_over_html() {
    let raw = b"From: Recruiter <recruiter@example.com>\r\nContent-Type: multipart/alternative; boundary=alt\r\n\r\n--alt\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nPlain version\r\n--alt\r\nContent-Type: text/html; charset=utf-8\r\n\r\n<b>HTML version</b>\r\n--alt--\r\n";
    let text = extract_text(raw);
    assert!(text.contains("Plain version"));
    assert!(!text.contains("HTML version"));
}

#[test]
fn rfc2047_subject_is_decoded() {
    let raw = b"Message-ID: <message@example.com>\r\nSubject: =?UTF-8?Q?Interview_invitation_=E2=9C=85?=\r\nFrom: Recruiter <recruiter@example.com>\r\nDate: Mon, 12 Oct 2026 10:00:00 +0000\r\n\r\nBody";
    let header = parse_header(raw, 17);
    assert!(header.subject.contains("Interview invitation"));
    assert!(header.subject.contains('✅'));
    assert_eq!(header.uid, 17);
    assert_eq!(header.message_id, "<message@example.com>");
}

#[test]
fn chunk_uids_splits_at_200() {
    let uids: Vec<u32> = (1..=450).collect();
    let chunks = chunk_uids(&uids);
    assert_eq!(
        chunks.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![200, 200, 50]
    );
}

#[test]
fn gmail_link_strips_brackets_and_encodes() {
    assert_eq!(
        gmail_link("<message+tag@example.com>"),
        "https://mail.google.com/mail/u/0/#search/rfc822msgid%3Amessage%2Btag%40example.com"
    );
}

#[test]
fn tls_config_builds_with_workspace_rustls_features() {
    super::imap::build_tls_config();
}

#[tokio::test]
#[ignore = "requires CVD_TEST_GMAIL_ADDRESS and CVD_TEST_GMAIL_APP_PASSWORD"]
async fn live_gmail_headers() {
    let account = MailAccount {
        host: "imap.gmail.com".into(),
        port: 993,
        address: std::env::var("CVD_TEST_GMAIL_ADDRESS").expect("test Gmail address is required"),
        password: std::env::var("CVD_TEST_GMAIL_APP_PASSWORD")
            .expect("test Gmail app password is required"),
    };
    let source = ImapSource::new(account, account::DEFAULT_FOLDER.into());
    source.check().await.unwrap();
    let since = chrono::Utc::now().date_naive() - chrono::Duration::days(30);
    let headers = source.recent_headers(since).await.unwrap();
    assert!(!headers.is_empty());
}

fn job(id: i64, company: &str, title: &str) -> JobCtx {
    JobCtx {
        id,
        company: company.into(),
        title: title.into(),
        url: String::new(),
        status: None,
    }
}

fn statuses() -> Vec<JobStatus> {
    vec![
        JobStatus {
            id: 1,
            name: "Applied".into(),
            color: StatusColor::Blue,
            position: 0,
        },
        JobStatus {
            id: 2,
            name: "Interview".into(),
            color: StatusColor::Purple,
            position: 1,
        },
    ]
}

fn probs(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

fn options(q: &Question) -> &[String] {
    match q {
        Question::Choice { options, .. } => options,
        other => panic!("expected a choice question, got {other:?}"),
    }
}

#[test]
fn questions_list_every_job_and_status_by_id() {
    let questions = build_questions(&[job(12, "Acme", "Engineer")], &statuses());

    for key in ["job_related", "matched_job", "status_changed", "new_status"] {
        assert!(questions.contains_key(key), "missing {key}");
    }
    let matched = options(&questions["matched_job"]);
    assert!(matched.contains(&"job_12".to_string()));
    assert!(matched.contains(&"no_match".to_string()));
    assert!(options(&questions["new_status"]).contains(&"status_2".to_string()));
}

#[test]
fn missing_company_still_gets_a_label() {
    assert_eq!(job_label("", "Engineer"), "Engineer");
    assert_eq!(job_label("", ""), "Untitled job");

    let questions = build_questions(&[job(12, "", "Engineer")], &statuses());
    let Question::Choice { criteria, .. } = &questions["matched_job"] else {
        panic!("matched_job must be a choice");
    };
    let label = &criteria["job_12"];
    assert!(label.contains("Engineer"));
}

#[test]
fn long_body_is_truncated_on_char_boundary() {
    let body = "ñ".repeat(10_000);
    let email = EmailInput::new("Subject", "sender@example.com", &body);

    assert_eq!(email.body.chars().count(), BODY_CHARS);
    assert!(build_state(&email, &[job(1, "Acme", "Engineer")]).contains("Subject"));
}

#[tokio::test]
async fn classify_parses_fake_answers() {
    let engine = FakeDecisionEngine::new(|_, key, _| match key {
        "job_related" => Answer::Noul { p: 0.95 },
        "matched_job" => Answer::Choice {
            choice: "job_12".into(),
            probabilities: probs(&[("job_12", 0.9), ("no_match", 0.1)]),
        },
        "status_changed" => Answer::Noul { p: 0.9 },
        "new_status" => Answer::Choice {
            choice: "status_2".into(),
            probabilities: probs(&[("status_1", 0.07), ("status_2", 0.93)]),
        },
        other => panic!("unexpected question {other}"),
    });
    let email = EmailInput::new("Interview", "recruiter@acme.com", "Let's talk.");

    let got = classify(&engine, &email, &[job(12, "Acme", "Engineer")], &statuses())
        .await
        .unwrap();

    assert_eq!(got.matched_job, Some(12));
    assert_eq!(got.new_status, Some((2, 0.93)));
    assert_eq!(got.job_related, 0.95);
    assert_eq!(got.status_changed, 0.9);
}

#[tokio::test]
async fn unknown_option_is_bad_response() {
    let engine = FakeDecisionEngine::new(|_, key, _| match key {
        "matched_job" => Answer::Choice {
            choice: "job_999".into(),
            probabilities: probs(&[("job_999", 1.0)]),
        },
        "new_status" => Answer::Choice {
            choice: "status_2".into(),
            probabilities: probs(&[("status_2", 1.0)]),
        },
        _ => Answer::Noul { p: 1.0 },
    });
    let email = EmailInput::new("Hi", "x@example.com", "");

    let got = classify(&engine, &email, &[job(12, "Acme", "Engineer")], &statuses()).await;

    assert!(matches!(got, Err(EngineError::BadResponse(_))), "{got:?}");
}

#[tokio::test]
async fn missing_answer_is_bad_response() {
    struct NoAnswers;

    #[async_trait]
    impl DecisionEngine for NoAnswers {
        async fn decide(
            &self,
            _state: &str,
            _questions: &BTreeMap<String, Question>,
        ) -> Result<BTreeMap<String, Answer>, EngineError> {
            Ok(BTreeMap::new())
        }
    }
    let email = EmailInput::new("Hi", "x@example.com", "");

    let got = classify(
        &NoAnswers,
        &email,
        &[job(12, "Acme", "Engineer")],
        &statuses(),
    )
    .await;

    assert!(matches!(got, Err(EngineError::BadResponse(_))), "{got:?}");
}

async fn chat_endpoint(server: &MockServer) -> ChatEndpoint {
    ChatEndpoint {
        base_url: format!("{}/v1", server.uri()),
        api_key: Some("sk-test".into()),
        model: "chat-model".into(),
    }
}

fn completion(content: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "choices": [{ "message": { "role": "assistant", "content": content } }]
    }))
}

async fn summarize_fixture(server: &MockServer) -> Result<String, AppError> {
    summarize(
        &reqwest::Client::new(),
        &chat_endpoint(server).await,
        &EmailInput::new("Interview", "recruiter@acme.com", "Let's talk."),
        &job(12, "Acme", "Engineer"),
        Some("Applied"),
        "Interview",
    )
    .await
}

#[tokio::test]
async fn returns_trimmed_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer sk-test"))
        .respond_with(completion("  Recruiter confirmed your interview.  "))
        .mount(&server)
        .await;

    let got = summarize_fixture(&server).await.unwrap();

    assert_eq!(got, "Recruiter confirmed your interview.");
}

#[tokio::test]
async fn unauthorized_maps_to_provider_auth() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let got = summarize_fixture(&server).await;

    assert!(matches!(got, Err(AppError::ProviderAuth(_))), "{got:?}");
}

#[tokio::test]
async fn empty_content_is_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(completion("   "))
        .mount(&server)
        .await;

    let got = summarize_fixture(&server).await;

    assert!(
        matches!(got, Err(AppError::EngineUnavailable(_))),
        "{got:?}"
    );
}

#[tokio::test]
async fn folder_defaults_to_all_mail_and_round_trips() {
    let store = store().await;
    assert_eq!(account::folder(&store).await.unwrap(), "[Gmail]/All Mail");

    account::set_folder(&store, " INBOX ").await.unwrap();

    assert_eq!(account::folder(&store).await.unwrap(), "INBOX");
}

#[test]
fn message_id_search_quotes_the_id() {
    assert_eq!(
        super::imap::message_id_search("<abc@mail.gmail.com>"),
        r#"HEADER Message-ID "<abc@mail.gmail.com>""#
    );
    assert_eq!(
        super::imap::message_id_search(r#"<a"b\c>"#),
        r#"HEADER Message-ID "<a\"b\\c>""#
    );
}
