//! The 30-day Gmail scan: read unseen mail, classify it, then apply or queue status changes.

use chrono::{Duration, NaiveDate};
use serde_json::{json, Value};

use super::classify::{classify, EmailInput, JobCtx};
use super::decide::{decide, Decision};
use super::imap::{MailHeader, MailSource};
use super::summary::summarize;
use crate::commands::AppError;
use crate::engine::DecisionEngine;
use crate::providers::ChatEndpoint;
use crate::state::Emitter;
use crate::store::types::{JobDescription, JobStatus};
use crate::store::{NewProcessed, Outcome, ScanStatus, Store};

pub const SCAN_DAYS: i64 = 30;
pub const PROGRESS_EVENT: &str = "scan://progress";
pub const FINISHED_EVENT: &str = "scan://finished";

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
pub async fn begin_run(store: &Store) -> Result<i64, AppError> {
    let jobs = store.list_jds().await?;
    let statuses = store.list_job_statuses().await?;
    if jobs.is_empty() || statuses.is_empty() {
        return Err(AppError::Io("Add a job and a status first.".into()));
    }
    Ok(store.create_scan_run().await?)
}

/// Runs the scan for a run created by `begin_run`, and always finishes that run.
pub async fn run_scan(deps: ScanDeps<'_>, run_id: i64) -> Result<ScanSummary, AppError> {
    let mut summary = ScanSummary {
        run_id,
        ..ScanSummary::default()
    };

    match scan(&deps, &mut summary).await {
        Ok(()) => {
            deps.store
                .finish_scan_run(run_id, ScanStatus::Done, None, summary.checked as i64, summary.failed as i64)
                .await?;
            (deps.emit)(FINISHED_EVENT, finished_payload(&summary, "done", None));
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
            (deps.emit)(FINISHED_EVENT, finished_payload(&summary, "failed", Some(&message)));
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

    for (index, header) in headers.iter().enumerate() {
        if !processed.contains(&header.message_id) {
            handle_header(deps, header, summary).await?;
        }
        summary.checked = index + 1;
        (deps.emit)(PROGRESS_EVENT, json!({ "checked": summary.checked, "total": total }));
    }
    Ok(())
}

/// Records one new email. Per-email problems count as `failed` and leave the
/// email unrecorded, so the next scan retries it. Returns `Err` only for
/// failures that make the rest of the scan pointless.
async fn handle_header(deps: &ScanDeps<'_>, header: &MailHeader, summary: &mut ScanSummary) -> Result<(), AppError> {
    let Ok(body) = deps.mail.body_text(header.uid).await else {
        summary.failed += 1;
        return Ok(());
    };

    // Re-read per email so a status applied earlier in this scan is seen here.
    let jobs = deps.store.list_jds().await?;
    let statuses = deps.store.list_job_statuses().await?;
    let contexts = job_contexts(&jobs, &statuses);
    let email = EmailInput::new(&header.subject, &header.sender, &body);

    let classified = match classify(deps.engine, &email, &contexts, &statuses).await {
        Ok(c) => c,
        Err(crate::engine::EngineError::BadResponse(_) | crate::engine::EngineError::ContextOverflow) => {
            summary.failed += 1;
            return Ok(());
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
        run_id: summary.run_id,
        received_at: header.received_at.clone(),
        subject: header.subject.clone(),
        sender: header.sender.clone(),
        matched_job_id: None,
        previous_status_id: None,
        suggested_status_id: None,
        confidence: None,
        summary: None,
        outcome: Outcome::Ignored,
    };

    let (job_id, next_status, confidence, outcome) = match decision {
        Decision::Ignored => {
            deps.store.record_processed(&row, None).await?;
            return Ok(());
        }
        Decision::NoChange { job_id } => (job_id, None, None, Outcome::NoChange),
        Decision::Applied { job_id, status_id, confidence } => {
            (job_id, Some(status_id), Some(confidence), Outcome::Applied)
        }
        Decision::PendingReview { job_id, status_id, confidence } => {
            (job_id, Some(status_id), Some(confidence), Outcome::PendingReview)
        }
    };

    let Some(job) = contexts.iter().find(|c| c.id == job_id) else {
        summary.failed += 1;
        return Ok(());
    };
    let previous_name = status_name(&statuses, current);
    let next_name = status_name(&statuses, next_status).unwrap_or("unchanged");

    let text = match summarize(deps.http, deps.chat, &email, job, previous_name, next_name).await {
        Ok(text) => text,
        Err(e @ (AppError::ProviderAuth(_) | AppError::ProviderCredits(_) | AppError::ProviderUnavailable(_))) => {
            return Err(e);
        }
        Err(_) => {
            summary.failed += 1;
            return Ok(());
        }
    };

    row.matched_job_id = Some(job_id);
    row.previous_status_id = current;
    row.suggested_status_id = next_status;
    row.confidence = confidence;
    row.summary = Some(text);
    row.outcome = outcome;
    let apply_status = if outcome == Outcome::Applied { next_status } else { None };
    deps.store.record_processed(&row, apply_status).await?;

    summary.job_related += 1;
    match outcome {
        Outcome::Applied => summary.applied += 1,
        Outcome::PendingReview => summary.pending += 1,
        Outcome::NoChange => summary.no_change += 1,
        Outcome::Ignored | Outcome::Dismissed => {}
    }
    Ok(())
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
    statuses.iter().find(|s| s.id == id).map(|s| s.name.as_str())
}

fn finished_payload(summary: &ScanSummary, status: &str, error: Option<&str>) -> Value {
    json!({
        "run_id": summary.run_id,
        "status": status,
        "error": error,
        "checked": summary.checked,
        "job_related": summary.job_related,
        "applied": summary.applied,
        "pending": summary.pending,
        "no_change": summary.no_change,
        "failed": summary.failed,
    })
}
