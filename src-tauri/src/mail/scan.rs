//! The 30-day Gmail scan: read unseen mail, classify it, then apply or queue status changes.

use chrono::{Duration, NaiveDate};
use futures::stream::{self, StreamExt};
use futures::FutureExt;
use serde_json::{json, Value};

use super::classify::{classify, EmailInput, JobCtx};
use super::decide::{decide, Decision, JOB_RELATED_THRESHOLD};
use super::imap::{MailHeader, MailSource};
use super::summary::summarize;
use crate::commands::AppError;
use crate::engine::DecisionEngine;
use crate::providers::ChatEndpoint;
use crate::state::{Emitter, JOB_UPDATED_EVENT};
use crate::store::types::{JobDescription, JobStatus};
use crate::store::{NewProcessed, Outcome, ScanSource, ScanStatus, Store};

pub const SCAN_DAYS: i64 = 30;
/// Most emails fetched, classified and summarised at the same time.
pub const MAX_CONCURRENT_EMAILS: usize = 4;
pub const PROGRESS_EVENT: &str = "scan://progress";
pub const FINISHED_EVENT: &str = "scan://finished";
/// A job-related inbox row was written or changed.
pub const INBOX_CHANGED_EVENT: &str = "inbox://changed";

pub struct ScanDeps<'a> {
    pub store: &'a Store,
    pub mail: &'a dyn MailSource,
    pub engine: &'a dyn DecisionEngine,
    pub chat: &'a ChatEndpoint,
    pub http: &'a reqwest::Client,
    pub emit: &'a Emitter,
    pub today: NaiveDate,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ScanSummary {
    pub run_id: i64,
    pub checked: usize,
    pub job_related: usize,
    pub applied: usize,
    pub pending: usize,
    pub no_change: usize,
    pub failed: usize,
}

/// Refuses to start without a job and a status, otherwise creates the run row.
pub async fn begin_run(store: &Store, source: ScanSource) -> Result<i64, AppError> {
    ensure_ready(store).await?;
    Ok(store.create_scan_run(source).await?)
}

/// A scan needs at least one job to match against and one status to suggest.
pub async fn ensure_ready(store: &Store) -> Result<(), AppError> {
    let jobs = store.list_jds().await?;
    let statuses = store.list_job_statuses().await?;
    if jobs.is_empty() || statuses.is_empty() {
        return Err(AppError::Io("Add a job and a status first.".into()));
    }
    Ok(())
}

/// Runs the scan for a run created by `begin_run`, and always finishes that run.
pub async fn run_scan(
    deps: ScanDeps<'_>,
    run_id: i64,
    source: ScanSource,
) -> Result<ScanSummary, AppError> {
    let mut summary = ScanSummary {
        run_id,
        ..ScanSummary::default()
    };
    let result = scan(&deps, &mut summary).await;
    finish_run(&deps, summary, source, result).await
}

/// Records how a run ended and announces it on `scan://finished`.
pub(crate) async fn finish_run(
    deps: &ScanDeps<'_>,
    summary: ScanSummary,
    source: ScanSource,
    result: Result<(), AppError>,
) -> Result<ScanSummary, AppError> {
    let run_id = summary.run_id;
    match result {
        Ok(()) => {
            deps.store
                .finish_scan_run(
                    run_id,
                    ScanStatus::Done,
                    None,
                    summary.checked as i64,
                    summary.failed as i64,
                )
                .await?;
            (deps.emit)(
                FINISHED_EVENT,
                finished_payload(&summary, source, "done", None),
            );
            Ok(summary)
        }
        Err(e) => {
            let message = e.to_string();
            deps.store
                .finish_scan_run(
                    run_id,
                    ScanStatus::Failed,
                    Some(&message),
                    summary.checked as i64,
                    summary.failed as i64,
                )
                .await?;
            (deps.emit)(
                FINISHED_EVENT,
                finished_payload(&summary, source, "failed", Some(&e)),
            );
            Err(e)
        }
    }
}

async fn scan(deps: &ScanDeps<'_>, summary: &mut ScanSummary) -> Result<(), AppError> {
    let since = deps.today - Duration::days(SCAN_DAYS);
    deps.mail.check().await?;
    let headers = deps.mail.recent_headers(since).await?;
    let total = headers.len();
    let processed = deps.store.processed_ids().await?;

    let (new, seen): (Vec<_>, Vec<_>) = headers
        .iter()
        .partition(|h| !processed.contains(&h.message_id));
    summary.checked = seen.len();
    (deps.emit)(
        PROGRESS_EVENT,
        json!({ "checked": summary.checked, "total": total }),
    );

    let run_id = summary.run_id;
    let mut pending = Vec::with_capacity(new.len());
    for header in new {
        pending.push(handle_header(deps, header, run_id).boxed());
    }
    let mut results = stream::iter(pending).buffer_unordered(MAX_CONCURRENT_EMAILS);
    while let Some(result) = results.next().await {
        match result? {
            Tally::Failed => summary.failed += 1,
            Tally::Ignored => {}
            Tally::Recorded(outcome) => {
                summary.job_related += 1;
                match outcome {
                    Outcome::Applied => summary.applied += 1,
                    Outcome::PendingReview => summary.pending += 1,
                    Outcome::NoChange => summary.no_change += 1,
                    Outcome::Ignored | Outcome::Dismissed => {}
                }
            }
        }
        summary.checked += 1;
        (deps.emit)(
            PROGRESS_EVENT,
            json!({ "checked": summary.checked, "total": total }),
        );
    }
    Ok(())
}

/// What happened to one email.
pub(crate) enum Tally {
    Failed,
    Ignored,
    Recorded(Outcome),
}

/// Records one new email. Per-email problems count as `failed` and leave the
/// email unrecorded, so the next scan retries it. Returns `Err` only for
/// failures that make the rest of the scan pointless.
async fn handle_header(
    deps: &ScanDeps<'_>,
    header: &MailHeader,
    run_id: i64,
) -> Result<Tally, AppError> {
    let Ok(body) = deps.mail.body_text(header.uid).await else {
        return Ok(Tally::Failed);
    };
    process_email(deps, &EmailMeta::from(header), &body, run_id).await
}

/// The parts of a message the pipeline records, whether it came from IMAP or
/// from a stored row being re-checked.
pub(crate) struct EmailMeta {
    pub message_id: String,
    pub subject: String,
    pub sender: String,
    pub received_at: String,
}

impl From<&MailHeader> for EmailMeta {
    fn from(h: &MailHeader) -> Self {
        Self {
            message_id: h.message_id.clone(),
            subject: h.subject.clone(),
            sender: h.sender.clone(),
            received_at: h.received_at.clone(),
        }
    }
}

/// Classifies one email body and records (or re-records) its row.
pub(crate) async fn process_email(
    deps: &ScanDeps<'_>,
    header: &EmailMeta,
    body: &str,
    run_id: i64,
) -> Result<Tally, AppError> {
    // Re-read per email so a status applied earlier in this scan is seen here.
    let jobs = deps.store.list_jds().await?;
    let statuses = deps.store.list_job_statuses().await?;
    let contexts = job_contexts(&jobs, &statuses);
    let email = EmailInput::new(&header.subject, &header.sender, body);

    let classified = match classify(deps.engine, &email, &contexts, &statuses).await {
        Ok(c) => c,
        Err(
            crate::engine::EngineError::BadResponse(_)
            | crate::engine::EngineError::ContextOverflow,
        ) => {
            return Ok(Tally::Failed);
        }
        Err(e) => return Err(e.into()),
    };

    let current = classified
        .matched_job
        .and_then(|id| jobs.iter().find(|j| j.id == id))
        .and_then(|j| j.status_id);
    let decision = decide(&classified, current);

    let mut row = NewProcessed {
        message_id: header.message_id.clone(),
        run_id,
        received_at: header.received_at.clone(),
        subject: header.subject.clone(),
        sender: header.sender.clone(),
        matched_job_id: None,
        previous_status_id: None,
        suggested_status_id: None,
        confidence: None,
        summary: None,
        outcome: Outcome::Ignored,
        job_related: Some(classified.job_related),
        body: (classified.job_related >= JOB_RELATED_THRESHOLD).then(|| email.body.clone()),
    };

    let (job_id, next_status, confidence, outcome) = match decision {
        Decision::Ignored => {
            deps.store.record_processed(&row, None).await?;
            return Ok(Tally::Ignored);
        }
        Decision::NoChange { job_id } => (job_id, None, None, Outcome::NoChange),
        Decision::Applied {
            job_id,
            status_id,
            confidence,
        } => (job_id, Some(status_id), Some(confidence), Outcome::Applied),
        Decision::PendingReview {
            job_id,
            status_id,
            confidence,
        } => (
            job_id,
            Some(status_id),
            Some(confidence),
            Outcome::PendingReview,
        ),
    };

    let Some(job) = contexts.iter().find(|c| c.id == job_id) else {
        return Ok(Tally::Failed);
    };
    let previous_name = status_name(&statuses, current);
    let next_name = status_name(&statuses, next_status).unwrap_or("unchanged");

    let text = match summarize(deps.http, deps.chat, &email, job, previous_name, next_name).await {
        Ok(text) => text,
        Err(
            e @ (AppError::ProviderAuth(_)
            | AppError::ProviderCredits(_)
            | AppError::ProviderUnavailable(_)),
        ) => {
            return Err(e);
        }
        Err(_) => {
            return Ok(Tally::Failed);
        }
    };

    row.matched_job_id = Some(job_id);
    row.previous_status_id = current;
    row.suggested_status_id = next_status;
    row.confidence = confidence;
    row.summary = Some(text);
    row.outcome = outcome;
    let apply_status = if outcome == Outcome::Applied {
        next_status
    } else {
        None
    };
    deps.store.record_processed(&row, apply_status).await?;
    (deps.emit)(INBOX_CHANGED_EVENT, json!({ "message_id": row.message_id }));

    // Notify the main window after the transaction commits so its job signal
    // reflects the classifier's status update without a manual refresh.
    if let Some(status_id) = apply_status {
        (deps.emit)(
            JOB_UPDATED_EVENT,
            json!({ "job_id": job_id, "status_id": status_id }),
        );
    }

    Ok(Tally::Recorded(outcome))
}

fn job_contexts(jobs: &[JobDescription], statuses: &[JobStatus]) -> Vec<JobCtx> {
    jobs.iter()
        .map(|j| JobCtx {
            id: j.id,
            company: j.company.clone(),
            title: j.title.clone(),
            url: j.url.clone(),
            status: status_name(statuses, j.status_id).map(str::to_string),
        })
        .collect()
}

fn status_name(statuses: &[JobStatus], id: Option<i64>) -> Option<&str> {
    let id = id?;
    statuses
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.name.as_str())
}

fn finished_payload(
    summary: &ScanSummary,
    source: ScanSource,
    status: &str,
    error: Option<&AppError>,
) -> Value {
    json!({
        "run_id": summary.run_id,
        "source": source.as_str(),
        "status": status,
        "error": error.map(AppError::to_string),
        "error_kind": error.map(AppError::kind),
        "checked": summary.checked,
        "job_related": summary.job_related,
        "applied": summary.applied,
        "pending": summary.pending,
        "no_change": summary.no_change,
        "failed": summary.failed,
    })
}
