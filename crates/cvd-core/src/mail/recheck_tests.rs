use std::sync::Arc;

use wiremock::MockServer;

use super::recheck::{needs_recheck, run_recheck, RECHECK_KEY};
use super::scan::ScanDeps;
use super::test_support::*;
use crate::engine::FakeDecisionEngine;
use crate::providers::ChatEndpoint;
use crate::services::AppError;
use crate::state::Emitter;
use crate::store::{NewProcessed, Outcome, ScanSource, Store};

async fn recheck(
    store: &Store,
    mail: &FakeMail,
    engine: &FakeDecisionEngine,
    chat: &ChatEndpoint,
) -> Result<(), AppError> {
    let emit: Emitter = Arc::new(|_: &str, _: serde_json::Value| {});
    let http = reqwest::Client::new();
    let run_id = store.create_scan_run(ScanSource::Recheck).await?;
    run_recheck(
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
    .map(|_| ())
}

/// An ignored row as an earlier scan left it, with the given score and body.
async fn ignored(store: &Store, message_id: &str, job_related: Option<f64>, body: Option<&str>) {
    let run = store.create_scan_run(ScanSource::Watch).await.unwrap();
    store
        .record_processed(
            &NewProcessed {
                message_id: message_id.into(),
                run_id: run,
                received_at: "2026-10-10T09:00:00+00:00".into(),
                subject: "Your application".into(),
                sender: "recruiter@globex.com".into(),
                matched_job_id: None,
                previous_status_id: None,
                suggested_status_id: None,
                confidence: None,
                summary: None,
                outcome: Outcome::Ignored,
                job_related,
                body: body.map(str::to_string),
            },
            None,
        )
        .await
        .unwrap();
    store.mark_all_read().await.unwrap();
}

async fn watermark(store: &Store) -> Option<String> {
    store.get_setting(RECHECK_KEY).await.unwrap()
}

#[tokio::test]
async fn first_run_initialises_watermark_without_rechecking() {
    let (store, job, _, _) = seeded().await;

    assert!(!needs_recheck(&store).await.unwrap());
    assert_eq!(watermark(&store).await, Some(job.to_string()));
}

#[tokio::test]
async fn new_job_triggers_recheck() {
    let (store, _, _, _) = seeded().await;
    needs_recheck(&store).await.unwrap();

    store
        .create_jd("Designer", "Globex", "raw", "", None)
        .await
        .unwrap();

    assert!(needs_recheck(&store).await.unwrap());
}

#[tokio::test]
async fn recheck_rematches_stored_body_without_imap() {
    let server = MockServer::start().await;
    let chat = chat_endpoint(&server).await;
    let (store, _, _, interviewing) = seeded().await;
    needs_recheck(&store).await.unwrap();
    ignored(
        &store,
        "<g>",
        Some(0.9),
        Some("Globex would like to interview you"),
    )
    .await;
    let globex = store
        .create_jd("Designer", "Globex", "raw", "", None)
        .await
        .unwrap();
    let mail = FakeMail::new(vec![]);
    let engine = scripted(
        0.95,
        format!("job_{globex}"),
        0.95,
        format!("status_{interviewing}"),
        0.6,
    );

    recheck(&store, &mail, &engine, &chat).await.unwrap();

    let row = store.get_processed("<g>").await.unwrap().unwrap();
    assert_eq!(row.outcome, Outcome::PendingReview);
    assert_eq!(row.matched_job_id, Some(globex));
    assert_eq!(row.read_at, None);
    assert!(mail.fetched().is_empty());
    assert_eq!(watermark(&store).await, Some(globex.to_string()));
}

#[tokio::test]
async fn recheck_refetches_legacy_row() {
    let server = MockServer::start().await;
    let chat = chat_endpoint(&server).await;
    let (store, job, _, interviewing) = seeded().await;
    ignored(&store, "<legacy>", None, None).await;
    let mail = FakeMail::new(vec![header(7, "<legacy>")]);
    let engine = scripted(
        0.95,
        format!("job_{job}"),
        0.95,
        format!("status_{interviewing}"),
        0.6,
    );

    recheck(&store, &mail, &engine, &chat).await.unwrap();

    assert_eq!(mail.fetched(), vec![7]);
    let row = store.get_processed("<legacy>").await.unwrap().unwrap();
    assert_eq!(row.job_related, Some(0.95));
    assert_eq!(row.outcome, Outcome::PendingReview);
}

#[tokio::test]
async fn recheck_skips_missing_legacy_message() {
    let (store, _, _, _) = seeded().await;
    ignored(&store, "<gone>", None, None).await;
    let mail = FakeMail::new(vec![]);

    recheck(&store, &mail, &ignoring(), &unreachable_chat())
        .await
        .unwrap();

    let row = store.get_processed("<gone>").await.unwrap().unwrap();
    assert_eq!(row.outcome, Outcome::Ignored);
    assert_eq!(row.job_related, None);
}

#[tokio::test]
async fn unrelated_rows_are_not_rechecked() {
    let (store, _, _, _) = seeded().await;
    ignored(&store, "<newsletter>", Some(0.1), None).await;
    let mail = FakeMail::new(vec![]);
    let engine = ignoring();

    recheck(&store, &mail, &engine, &unreachable_chat())
        .await
        .unwrap();

    assert_eq!(engine.calls(), 0);
}

#[tokio::test]
async fn failed_recheck_keeps_watermark() {
    let (store, _, _, _) = seeded().await;
    needs_recheck(&store).await.unwrap();
    let before = watermark(&store).await;
    ignored(&store, "<g>", Some(0.9), Some("body")).await;
    store
        .create_jd("Designer", "Globex", "raw", "", None)
        .await
        .unwrap();
    let mail = FakeMail::new(vec![]);
    let engine = ignoring();
    engine.fail_on_call(1);

    assert!(recheck(&store, &mail, &engine, &unreachable_chat())
        .await
        .is_err());
    assert_eq!(watermark(&store).await, before);
}

#[tokio::test]
async fn legacy_fetch_error_counts_as_failed() {
    let (store, _, _, _) = seeded().await;
    ignored(&store, "<broken>", None, None).await;
    let mut mail = FakeMail::new(vec![header(9, "<broken>")]);
    mail.bodies.clear();

    recheck(&store, &mail, &ignoring(), &unreachable_chat())
        .await
        .unwrap();

    let row = store.get_processed("<broken>").await.unwrap().unwrap();
    assert_eq!(row.outcome, Outcome::Ignored);
}
