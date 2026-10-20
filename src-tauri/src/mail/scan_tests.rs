use serde_json::json;
use wiremock::MockServer;

use super::scan::{FINISHED_EVENT, INBOX_CHANGED_EVENT};
use super::test_support::*;
use crate::commands::AppError;
use crate::store::{NewProcessed, Outcome, ScanSource, ScanStatus};

#[tokio::test]
async fn ignored_email_stores_score_without_body() {
    let (store, _, _, _) = seeded().await;
    let mail = FakeMail::new(vec![header(1, "<a>")]);

    run(&store, &mail, &ignoring(), &unreachable_chat())
        .await
        .unwrap();

    let row = store.get_processed("<a>").await.unwrap().unwrap();
    assert_eq!(row.job_related, Some(0.1));
    assert_eq!(row.body, None);
}

#[tokio::test]
async fn job_related_unmatched_stores_body() {
    let (store, _, _, interviewing) = seeded().await;
    let mail = FakeMail::new(vec![header(1, "<a>")]);
    let engine = scripted(
        0.9,
        "no_match".into(),
        0.1,
        format!("status_{interviewing}"),
        0.5,
    );

    run(&store, &mail, &engine, &unreachable_chat())
        .await
        .unwrap();

    let row = store.get_processed("<a>").await.unwrap().unwrap();
    assert_eq!(row.outcome, Outcome::Ignored);
    assert_eq!(row.job_related, Some(0.9));
    assert_eq!(row.body.as_deref(), Some("body of 1"));
}

#[tokio::test]
async fn matched_email_stores_body_and_emits_inbox_changed() {
    let server = MockServer::start().await;
    let chat = chat_endpoint(&server).await;
    let (store, job, _, interviewing) = seeded().await;
    let mail = FakeMail::new(vec![header(1, "<a>")]);
    let engine = scripted(
        0.95,
        format!("job_{job}"),
        0.95,
        format!("status_{interviewing}"),
        0.9,
    );
    let (emit, events) = capturing();

    run_with(&store, &mail, &engine, &chat, ScanSource::Watch, &emit)
        .await
        .unwrap();

    let row = store.get_processed("<a>").await.unwrap().unwrap();
    assert_eq!(row.body.as_deref(), Some("body of 1"));
    let changed: Vec<_> = events
        .lock()
        .unwrap()
        .iter()
        .filter(|(name, _)| name == INBOX_CHANGED_EVENT)
        .map(|(_, payload)| payload.clone())
        .collect();
    assert_eq!(changed, vec![json!({ "message_id": "<a>" })]);
}

#[tokio::test]
async fn ignored_email_does_not_emit_inbox_changed() {
    let (store, _, _, _) = seeded().await;
    let mail = FakeMail::new(vec![header(1, "<a>")]);
    let (emit, events) = capturing();

    run_with(
        &store,
        &mail,
        &ignoring(),
        &unreachable_chat(),
        ScanSource::Watch,
        &emit,
    )
    .await
    .unwrap();

    assert!(!events
        .lock()
        .unwrap()
        .iter()
        .any(|(name, _)| name == INBOX_CHANGED_EVENT));
}

#[tokio::test]
async fn finished_payload_carries_source() {
    let (store, _, _, _) = seeded().await;
    let mail = FakeMail::new(vec![]);
    let (emit, events) = capturing();

    run_with(
        &store,
        &mail,
        &ignoring(),
        &unreachable_chat(),
        ScanSource::Watch,
        &emit,
    )
    .await
    .unwrap();

    let events = events.lock().unwrap();
    let (_, finished) = events
        .iter()
        .find(|(name, _)| name == FINISHED_EVENT)
        .unwrap();
    assert_eq!(finished["source"], "watch");
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
