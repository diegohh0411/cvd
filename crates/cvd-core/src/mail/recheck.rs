//! Re-classifies recent ignored emails when a job is added, so mail that
//! arrived before its job existed can still match it.

use futures::stream::{self, StreamExt};
use futures::FutureExt;
use serde_json::json;

use super::imap::MailError;
use super::scan::{
    finish_run, process_email, EmailMeta, ScanDeps, ScanSummary, Tally, MAX_CONCURRENT_EMAILS,
    PROGRESS_EVENT,
};
use crate::services::AppError;
use crate::store::{Outcome, ProcessedEmail, ScanSource, TenantStore as Store};

/// Settings key holding the highest job id already re-checked.
pub const RECHECK_KEY: &str = "mail_recheck_job_id";

/// Whether a job was added since the last re-check. The first call only
/// records the current jobs, so upgrading does not re-classify all history.
pub async fn needs_recheck(store: &Store) -> Result<bool, AppError> {
    let max = store.max_job_id().await?;
    match store.get_setting(RECHECK_KEY).await? {
        Some(stored) => Ok(max > stored.parse::<i64>().unwrap_or(0)),
        None => {
            store.set_setting(RECHECK_KEY, &max.to_string()).await?;
            Ok(false)
        }
    }
}

/// Re-runs the pipeline over every re-check candidate for a run created with
/// source `recheck`. The watermark advances only if the whole run succeeds.
pub async fn run_recheck(deps: ScanDeps<'_>, run_id: i64) -> Result<ScanSummary, AppError> {
    let mut summary = ScanSummary {
        run_id,
        ..ScanSummary::default()
    };
    let max = deps.store.max_job_id().await?;
    let result = recheck(&deps, &mut summary).await;
    let ok = result.is_ok();
    let summary = finish_run(&deps, summary, ScanSource::Recheck, result).await?;
    if ok {
        deps.store
            .set_setting(RECHECK_KEY, &max.to_string())
            .await?;
    }
    Ok(summary)
}

async fn recheck(deps: &ScanDeps<'_>, summary: &mut ScanSummary) -> Result<(), AppError> {
    let candidates = deps.store.recheck_candidates().await?;
    let total = candidates.len();
    let run_id = summary.run_id;
    let mut pending = Vec::with_capacity(total);
    for row in &candidates {
        pending.push(recheck_row(deps, row, run_id).boxed());
    }
    let mut results = stream::iter(pending).buffer_unordered(MAX_CONCURRENT_EMAILS);
    while let Some(result) = results.next().await {
        match result? {
            None | Some(Tally::Ignored) => {}
            Some(Tally::Failed) => summary.failed += 1,
            Some(Tally::Recorded(outcome)) => {
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

/// Re-classifies one candidate. `None` means its body is gone from the mailbox.
async fn recheck_row(
    deps: &ScanDeps<'_>,
    row: &ProcessedEmail,
    run_id: i64,
) -> Result<Option<Tally>, AppError> {
    let body = match row.body.clone() {
        Some(body) => body,
        // Rows from before bodies were stored. A message that cannot be
        // read is skipped; a broken connection stops the re-check.
        None => match deps.mail.body_by_message_id(&row.message_id).await {
            Ok(Some(body)) => body,
            Ok(None) => return Ok(None),
            Err(MailError::Protocol(_)) => return Ok(Some(Tally::Failed)),
            Err(e) => return Err(e.into()),
        },
    };
    let meta = EmailMeta {
        message_id: row.message_id.clone(),
        subject: row.subject.clone(),
        sender: row.sender.clone(),
        received_at: row.received_at.clone(),
    };
    process_email(deps, &meta, &body, run_id).await.map(Some)
}
