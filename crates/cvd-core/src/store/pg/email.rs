use super::super::{email::*, Result, StoreError, UserId};
use super::{optional_timestamp, timestamp};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, Row};
use std::collections::HashSet;
fn outcome_str(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Ignored => "ignored",
        Outcome::NoChange => "no_change",
        Outcome::Applied => "applied",
        Outcome::PendingReview => "pending_review",
        Outcome::Dismissed => "dismissed",
    }
}
fn parse_outcome(value: &str) -> Result<Outcome> {
    Ok(match value {
        "ignored" => Outcome::Ignored,
        "no_change" => Outcome::NoChange,
        "applied" => Outcome::Applied,
        "pending_review" => Outcome::PendingReview,
        "dismissed" => Outcome::Dismissed,
        other => {
            return Err(StoreError::Invalid(format!(
                "processed email outcome {other}"
            )))
        }
    })
}
fn scan_status_str(status: ScanStatus) -> &'static str {
    match status {
        ScanStatus::Running => "running",
        ScanStatus::Done => "done",
        ScanStatus::Failed => "failed",
    }
}
fn parse_scan_status(value: &str) -> Result<ScanStatus> {
    Ok(match value {
        "running" => ScanStatus::Running,
        "done" => ScanStatus::Done,
        "failed" => ScanStatus::Failed,
        other => return Err(StoreError::Invalid(format!("scan status {other}"))),
    })
}

fn processed_from_row(r: &PgRow) -> Result<ProcessedEmail> {
    Ok(ProcessedEmail {
        message_id: r.get("message_id"),
        run_id: r.get("run_id"),
        processed_at: timestamp(r, "processed_at"),
        received_at: timestamp(r, "received_at"),
        subject: r.get("subject"),
        sender: r.get("sender"),
        matched_job_id: r.get("matched_job_id"),
        previous_status_id: r.get("previous_status_id"),
        suggested_status_id: r.get("suggested_status_id"),
        confidence: r.get("confidence"),
        summary: r.get("summary"),
        outcome: parse_outcome(r.get("outcome"))?,
        job_related: r.get("job_related"),
        body: r.get("body"),
        read_at: optional_timestamp(r, "read_at"),
        undone_at: optional_timestamp(r, "undone_at"),
    })
}

fn scan_run_from_row(r: &PgRow) -> Result<ScanRun> {
    Ok(ScanRun {
        id: r.get("id"),
        started_at: timestamp(r, "started_at"),
        finished_at: optional_timestamp(r, "finished_at"),
        status: parse_scan_status(r.get("status"))?,
        error: r.get("error"),
        checked: r.get("checked"),
        failed: r.get("failed"),
    })
}

pub(in crate::store) async fn create_scan_run(
    conn: &mut PgConnection,
    user: UserId,
    source: ScanSource,
) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO scan_run (user_id, status, source) VALUES ($1, 'running', $2) RETURNING id",
    )
    .bind(user.0)
    .bind(source.as_str())
    .fetch_one(&mut *conn)
    .await?)
}

pub(in crate::store) async fn finish_scan_run(
    conn: &mut PgConnection,
    user: UserId,
    run_id: i64,
    status: ScanStatus,
    error: Option<&str>,
    checked: i64,
    failed: i64,
) -> Result<()> {
    sqlx::query(
            "UPDATE scan_run SET finished_at = CURRENT_TIMESTAMP, status = $2, error = $3, checked = $4, failed = $5 WHERE user_id = $1 AND id = $6",
        ).bind(user.0)
        .bind(scan_status_str(status))
        .bind(error)
        .bind(checked)
        .bind(failed)
        .bind(run_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub(in crate::store) async fn get_scan_run(
    conn: &mut PgConnection,
    user: UserId,
    run_id: i64,
) -> Result<Option<ScanRun>> {
    let row = sqlx::query(
        "SELECT * FROM (SELECT * FROM scan_run WHERE user_id = $1) scan_run WHERE id = $2",
    )
    .bind(user.0)
    .bind(run_id)
    .fetch_optional(&mut *conn)
    .await?;
    row.as_ref().map(scan_run_from_row).transpose()
}

pub(in crate::store) async fn processed_ids(
    conn: &mut PgConnection,
    user: UserId,
) -> Result<HashSet<String>> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT message_id FROM (SELECT * FROM processed_email WHERE user_id = $1) processed_email",
    )
    .bind(user.0)
    .fetch_all(&mut *conn)
    .await?;
    Ok(ids.into_iter().collect())
}

/// Stores a message and, when requested, updates its matched job atomically.
/// A message already stored (a re-check) is overwritten in place, and
/// becomes unread again unless it is still ignored.
pub(in crate::store) async fn record_processed(
    conn: &mut PgConnection,
    user: UserId,
    p: &NewProcessed,
    apply_status: Option<i64>,
) -> Result<()> {
    sqlx::query(
            "INSERT INTO processed_email (user_id,
                message_id, run_id, received_at, subject, sender, matched_job_id,
                previous_status_id, suggested_status_id, confidence, summary, outcome,
                job_related, body
             ) VALUES ($1, $2, $3, $4::text::timestamptz, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
             ON CONFLICT(user_id, message_id) DO UPDATE SET
                run_id = excluded.run_id,


                processed_at = CASE WHEN excluded.outcome = 'ignored'
                    THEN processed_email.processed_at ELSE CURRENT_TIMESTAMP END,
                matched_job_id = excluded.matched_job_id,
                previous_status_id = excluded.previous_status_id,
                suggested_status_id = excluded.suggested_status_id,
                confidence = excluded.confidence,
                summary = excluded.summary,
                outcome = excluded.outcome,
                job_related = excluded.job_related,
                body = excluded.body,
                read_at = CASE WHEN excluded.outcome = 'ignored'
                    THEN processed_email.read_at ELSE NULL END",
        ).bind(user.0)
        .bind(&p.message_id)
        .bind(p.run_id)
        .bind(&p.received_at)
        .bind(&p.subject)
        .bind(&p.sender)
        .bind(p.matched_job_id)
        .bind(p.previous_status_id)
        .bind(p.suggested_status_id)
        .bind(p.confidence)
        .bind(&p.summary)
        .bind(outcome_str(p.outcome))
        .bind(p.job_related)
        .bind(&p.body)
        .execute(&mut *conn)
        .await?;
    if let (Some(job_id), Some(status_id)) = (p.matched_job_id, apply_status) {
        sqlx::query("UPDATE job_description SET status_id = $2 WHERE user_id = $1 AND id = $3")
            .bind(user.0)
            .bind(status_id)
            .bind(job_id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

pub(in crate::store) async fn get_processed(
    conn: &mut PgConnection,
    user: UserId,
    message_id: &str,
) -> Result<Option<ProcessedEmail>> {
    let row = sqlx::query("SELECT * FROM (SELECT * FROM processed_email WHERE user_id = $1) processed_email WHERE message_id = $2").bind(user.0)
            .bind(message_id)
            .fetch_optional(&mut *conn)
            .await?;
    row.as_ref().map(processed_from_row).transpose()
}

pub(in crate::store) async fn list_run_results(
    conn: &mut PgConnection,
    user: UserId,
    run_id: i64,
) -> Result<Vec<ProcessedEmail>> {
    let rows = sqlx::query(
            "SELECT * FROM (SELECT * FROM processed_email WHERE user_id = $1) processed_email WHERE run_id = $2 ORDER BY processed_at, message_id",
        ).bind(user.0)
        .bind(run_id)
        .fetch_all(&mut *conn)
        .await?;
    rows.iter().map(processed_from_row).collect()
}

/// Emails matched to one job, newest first.
pub(in crate::store) async fn list_job_emails(
    conn: &mut PgConnection,
    user: UserId,
    job_id: i64,
) -> Result<Vec<ProcessedEmail>> {
    let rows = sqlx::query(
            "SELECT * FROM (SELECT * FROM processed_email WHERE user_id = $1) processed_email WHERE matched_job_id = $2 ORDER BY received_at DESC, message_id",
        ).bind(user.0)
        .bind(job_id)
        .fetch_all(&mut *conn)
        .await?;
    rows.iter().map(processed_from_row).collect()
}

pub(in crate::store) async fn list_pending_reviews(
    conn: &mut PgConnection,
    user: UserId,
) -> Result<Vec<ProcessedEmail>> {
    let rows = sqlx::query(
            "SELECT * FROM (SELECT * FROM processed_email WHERE user_id = $1) processed_email WHERE outcome = 'pending_review' ORDER BY received_at DESC, message_id",
        ).bind(user.0)
        .fetch_all(&mut *conn)
        .await?;
    rows.iter().map(processed_from_row).collect()
}

/// Applies a saved suggestion, failing if any referenced record was removed.
pub(in crate::store) async fn apply_suggestion(
    conn: &mut PgConnection,
    user: UserId,
    message_id: &str,
) -> Result<ProcessedEmail> {
    let row = sqlx::query("SELECT * FROM (SELECT * FROM processed_email WHERE user_id = $1) processed_email WHERE message_id = $2 FOR UPDATE").bind(user.0)
            .bind(message_id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or(StoreError::NotFound)?;
    let processed = processed_from_row(&row)?;
    let job_id = processed.matched_job_id.ok_or(StoreError::NotFound)?;
    let status_id = processed.suggested_status_id.ok_or(StoreError::NotFound)?;
    let status_exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM (SELECT * FROM job_status WHERE user_id = $1) job_status WHERE id = $2)").bind(user.0)
                .bind(status_id)
                .fetch_one(&mut *conn)
                .await?;
    if !status_exists {
        return Err(StoreError::NotFound);
    }
    let current: Option<i64> =
            sqlx::query_scalar("SELECT status_id FROM (SELECT * FROM job_description WHERE user_id = $1) job_description WHERE id = $2 FOR UPDATE").bind(user.0)
                .bind(job_id)
                .fetch_optional(&mut *conn)
                .await?
                .flatten();
    let updated =
        sqlx::query("UPDATE job_description SET status_id = $2 WHERE user_id = $1 AND id = $3")
            .bind(user.0)
            .bind(status_id)
            .bind(job_id)
            .execute(&mut *conn)
            .await?;
    if updated.rows_affected() == 0 {
        return Err(StoreError::NotFound);
    }
    // Undo restores the status the job had when this was applied, which may
    // be one the user set by hand after the email arrived.
    sqlx::query(
        "UPDATE processed_email SET outcome = 'applied', read_at = CURRENT_TIMESTAMP,
                previous_status_id = $2 WHERE user_id = $1 AND message_id = $3",
    )
    .bind(user.0)
    .bind(current)
    .bind(message_id)
    .execute(&mut *conn)
    .await?;
    let row = sqlx::query("SELECT * FROM (SELECT * FROM processed_email WHERE user_id = $1) processed_email WHERE message_id = $2").bind(user.0)
            .bind(message_id)
            .fetch_one(&mut *conn)
            .await?;
    let result = processed_from_row(&row)?;
    Ok(result)
}

pub(in crate::store) async fn dismiss_suggestion(
    conn: &mut PgConnection,
    user: UserId,
    message_id: &str,
) -> Result<()> {
    sqlx::query(
            "UPDATE processed_email SET outcome = 'dismissed', read_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND message_id = $2",
        ).bind(user.0)
        .bind(message_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Job-related results, reviews first, newest processed first.
pub(in crate::store) async fn list_inbox(
    conn: &mut PgConnection,
    user: UserId,
    filter: InboxFilter,
) -> Result<Vec<ProcessedEmail>> {
    let condition = match filter {
        InboxFilter::All => "outcome != 'ignored'",
        InboxFilter::Review => "outcome = 'pending_review'",
        InboxFilter::Unread => "outcome != 'ignored' AND read_at IS NULL",
    };
    let rows = sqlx::query(&format!(
            "SELECT * FROM (SELECT * FROM processed_email WHERE user_id = $1) processed_email WHERE {condition}
             ORDER BY (outcome = 'pending_review') DESC, processed_at DESC, message_id"
        )).bind(user.0)
        .fetch_all(&mut *conn)
        .await?;
    rows.iter().map(processed_from_row).collect()
}

pub(in crate::store) async fn unread_count(conn: &mut PgConnection, user: UserId) -> Result<i64> {
    Ok(sqlx::query_scalar(
            "SELECT COUNT(*) FROM (SELECT * FROM processed_email WHERE user_id = $1) processed_email WHERE outcome != 'ignored' AND read_at IS NULL",
        ).bind(user.0)
        .fetch_one(&mut *conn)
        .await?)
}

pub(in crate::store) async fn mark_read(
    conn: &mut PgConnection,
    user: UserId,
    message_id: &str,
) -> Result<()> {
    sqlx::query(
            "UPDATE processed_email SET read_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND message_id = $2 AND read_at IS NULL",
        ).bind(user.0)
        .bind(message_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub(in crate::store) async fn mark_all_read(conn: &mut PgConnection, user: UserId) -> Result<()> {
    sqlx::query("UPDATE processed_email SET read_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND read_at IS NULL").bind(user.0)
            .execute(&mut *conn)
            .await?;
    Ok(())
}

/// Restores the status an auto-apply replaced, unless the job moved since.
pub(in crate::store) async fn undo_applied(
    conn: &mut PgConnection,
    user: UserId,
    message_id: &str,
) -> Result<ProcessedEmail> {
    let row = sqlx::query("SELECT * FROM (SELECT * FROM processed_email WHERE user_id = $1) processed_email WHERE message_id = $2 FOR UPDATE").bind(user.0)
            .bind(message_id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or(StoreError::NotFound)?;
    let processed = processed_from_row(&row)?;
    let job_id = processed.matched_job_id.ok_or(StoreError::NotFound)?;
    let current: Option<Option<i64>> =
            sqlx::query_scalar("SELECT status_id FROM (SELECT * FROM job_description WHERE user_id = $1) job_description WHERE id = $2 FOR UPDATE").bind(user.0)
                .bind(job_id)
                .fetch_optional(&mut *conn)
                .await?;
    let current = current.ok_or(StoreError::NotFound)?;
    if processed.outcome != Outcome::Applied
        || processed.undone_at.is_some()
        || current != processed.suggested_status_id
    {
        return Err(StoreError::Invalid(UNDO_BLOCKED.into()));
    }
    sqlx::query("UPDATE job_description SET status_id = $2 WHERE user_id = $1 AND id = $3")
        .bind(user.0)
        .bind(processed.previous_status_id)
        .bind(job_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
            "UPDATE processed_email SET undone_at = CURRENT_TIMESTAMP,
                read_at = COALESCE(read_at, CURRENT_TIMESTAMP) WHERE user_id = $1 AND message_id = $2",
        ).bind(user.0)
        .bind(message_id)
        .execute(&mut *conn)
        .await?;
    let row = sqlx::query("SELECT * FROM (SELECT * FROM processed_email WHERE user_id = $1) processed_email WHERE message_id = $2").bind(user.0)
            .bind(message_id)
            .fetch_one(&mut *conn)
            .await?;
    let result = processed_from_row(&row)?;
    Ok(result)
}

/// Recent ignored emails that looked job-related, or predate the score.
pub(in crate::store) async fn recheck_candidates(
    conn: &mut PgConnection,
    user: UserId,
) -> Result<Vec<ProcessedEmail>> {
    let rows = sqlx::query(
        "SELECT * FROM (SELECT * FROM processed_email WHERE user_id = $1) processed_email
             WHERE outcome = 'ignored'
               AND processed_at >= CURRENT_TIMESTAMP - INTERVAL '30 days'
               AND (job_related >= 0.5 OR job_related IS NULL)
             ORDER BY processed_at, message_id",
    )
    .bind(user.0)
    .fetch_all(&mut *conn)
    .await?;
    rows.iter().map(processed_from_row).collect()
}

pub(in crate::store) async fn max_job_id(conn: &mut PgConnection, user: UserId) -> Result<i64> {
    Ok(
            sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM (SELECT * FROM job_description WHERE user_id = $1) job_description").bind(user.0)
                .fetch_one(&mut *conn)
                .await?,
        )
}
