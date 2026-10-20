//! Gmail scan lifecycle and deduplicated processed-message records.
use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

use super::{Result, Store, StoreError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ignored,
    NoChange,
    Applied,
    PendingReview,
    Dismissed,
}

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

#[derive(Debug, Clone, PartialEq)]
pub struct NewProcessed {
    pub message_id: String,
    pub run_id: i64,
    pub received_at: String,
    pub subject: String,
    pub sender: String,
    pub matched_job_id: Option<i64>,
    pub previous_status_id: Option<i64>,
    pub suggested_status_id: Option<i64>,
    pub confidence: Option<f64>,
    pub summary: Option<String>,
    pub outcome: Outcome,
    /// System One's `job_related` probability.
    pub job_related: Option<f64>,
    /// Truncated body, kept only for job-related mail.
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessedEmail {
    pub message_id: String,
    pub run_id: i64,
    pub processed_at: String,
    pub received_at: String,
    pub subject: String,
    pub sender: String,
    pub matched_job_id: Option<i64>,
    pub previous_status_id: Option<i64>,
    pub suggested_status_id: Option<i64>,
    pub confidence: Option<f64>,
    pub summary: Option<String>,
    pub outcome: Outcome,
    pub job_related: Option<f64>,
    pub body: Option<String>,
    /// `None` until the row is read in the inbox.
    pub read_at: Option<String>,
    /// Set when an auto-applied status was undone.
    pub undone_at: Option<String>,
}

/// What started a scan run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanSource {
    Watch,
    Recheck,
    Manual,
}

impl ScanSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ScanSource::Watch => "watch",
            ScanSource::Recheck => "recheck",
            ScanSource::Manual => "manual",
        }
    }
}

/// Which inbox rows to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InboxFilter {
    All,
    Review,
    Unread,
}

/// Shown when Undo would overwrite a status the user set by hand.
pub const UNDO_BLOCKED: &str = "The status was changed after this update.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanStatus {
    Running,
    Done,
    Failed,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanRun {
    pub id: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: ScanStatus,
    pub error: Option<String>,
    pub checked: i64,
    pub failed: i64,
}

fn processed_from_row(r: &SqliteRow) -> Result<ProcessedEmail> {
    Ok(ProcessedEmail {
        message_id: r.get("message_id"),
        run_id: r.get("run_id"),
        processed_at: r.get("processed_at"),
        received_at: r.get("received_at"),
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
        read_at: r.get("read_at"),
        undone_at: r.get("undone_at"),
    })
}

fn scan_run_from_row(r: &SqliteRow) -> Result<ScanRun> {
    Ok(ScanRun {
        id: r.get("id"),
        started_at: r.get("started_at"),
        finished_at: r.get("finished_at"),
        status: parse_scan_status(r.get("status"))?,
        error: r.get("error"),
        checked: r.get("checked"),
        failed: r.get("failed"),
    })
}

impl Store {
    pub async fn create_scan_run(&self, source: ScanSource) -> Result<i64> {
        Ok(
            sqlx::query("INSERT INTO scan_run (status, source) VALUES ('running', ?)")
                .bind(source.as_str())
                .execute(&self.pool)
                .await?
                .last_insert_rowid(),
        )
    }

    pub async fn finish_scan_run(
        &self,
        run_id: i64,
        status: ScanStatus,
        error: Option<&str>,
        checked: i64,
        failed: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE scan_run SET finished_at = CURRENT_TIMESTAMP, status = ?, error = ?, checked = ?, failed = ? WHERE id = ?",
        )
        .bind(scan_status_str(status))
        .bind(error)
        .bind(checked)
        .bind(failed)
        .bind(run_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_scan_run(&self, run_id: i64) -> Result<Option<ScanRun>> {
        let row = sqlx::query("SELECT * FROM scan_run WHERE id = ?")
            .bind(run_id)
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(scan_run_from_row).transpose()
    }

    pub async fn processed_ids(&self) -> Result<HashSet<String>> {
        let ids: Vec<String> = sqlx::query_scalar("SELECT message_id FROM processed_email")
            .fetch_all(&self.pool)
            .await?;
        Ok(ids.into_iter().collect())
    }

    /// Stores a message and, when requested, updates its matched job atomically.
    /// A message already stored (a re-check) is overwritten in place, and
    /// becomes unread again unless it is still ignored.
    pub async fn record_processed(
        &self,
        p: &NewProcessed,
        apply_status: Option<i64>,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO processed_email (
                message_id, run_id, received_at, subject, sender, matched_job_id,
                previous_status_id, suggested_status_id, confidence, summary, outcome,
                job_related, body
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(message_id) DO UPDATE SET
                run_id = excluded.run_id,
                processed_at = CURRENT_TIMESTAMP,
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
        )
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
        .execute(&mut *tx)
        .await?;
        if let (Some(job_id), Some(status_id)) = (p.matched_job_id, apply_status) {
            sqlx::query("UPDATE job_description SET status_id = ? WHERE id = ?")
                .bind(status_id)
                .bind(job_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_processed(&self, message_id: &str) -> Result<Option<ProcessedEmail>> {
        let row = sqlx::query("SELECT * FROM processed_email WHERE message_id = ?")
            .bind(message_id)
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(processed_from_row).transpose()
    }

    pub async fn list_run_results(&self, run_id: i64) -> Result<Vec<ProcessedEmail>> {
        let rows = sqlx::query(
            "SELECT * FROM processed_email WHERE run_id = ? ORDER BY processed_at, message_id",
        )
        .bind(run_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(processed_from_row).collect()
    }

    /// Emails matched to one job, newest first.
    pub async fn list_job_emails(&self, job_id: i64) -> Result<Vec<ProcessedEmail>> {
        let rows = sqlx::query(
            "SELECT * FROM processed_email WHERE matched_job_id = ? ORDER BY received_at DESC, message_id",
        )
        .bind(job_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(processed_from_row).collect()
    }

    pub async fn list_pending_reviews(&self) -> Result<Vec<ProcessedEmail>> {
        let rows = sqlx::query(
            "SELECT * FROM processed_email WHERE outcome = 'pending_review' ORDER BY received_at DESC, message_id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(processed_from_row).collect()
    }

    /// Applies a saved suggestion, failing if any referenced record was removed.
    pub async fn apply_suggestion(&self, message_id: &str) -> Result<ProcessedEmail> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT * FROM processed_email WHERE message_id = ?")
            .bind(message_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let processed = processed_from_row(&row)?;
        let job_id = processed.matched_job_id.ok_or(StoreError::NotFound)?;
        let status_id = processed.suggested_status_id.ok_or(StoreError::NotFound)?;
        let status_exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM job_status WHERE id = ?)")
                .bind(status_id)
                .fetch_one(&mut *tx)
                .await?;
        if !status_exists {
            return Err(StoreError::NotFound);
        }
        let updated = sqlx::query("UPDATE job_description SET status_id = ? WHERE id = ?")
            .bind(status_id)
            .bind(job_id)
            .execute(&mut *tx)
            .await?;
        if updated.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        sqlx::query(
            "UPDATE processed_email SET outcome = 'applied', read_at = CURRENT_TIMESTAMP WHERE message_id = ?",
        )
            .bind(message_id)
            .execute(&mut *tx)
            .await?;
        let row = sqlx::query("SELECT * FROM processed_email WHERE message_id = ?")
            .bind(message_id)
            .fetch_one(&mut *tx)
            .await?;
        let result = processed_from_row(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn dismiss_suggestion(&self, message_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE processed_email SET outcome = 'dismissed', read_at = CURRENT_TIMESTAMP WHERE message_id = ?",
        )
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Job-related results, reviews first, newest processed first.
    pub async fn list_inbox(&self, filter: InboxFilter) -> Result<Vec<ProcessedEmail>> {
        let condition = match filter {
            InboxFilter::All => "outcome != 'ignored'",
            InboxFilter::Review => "outcome = 'pending_review'",
            InboxFilter::Unread => "outcome != 'ignored' AND read_at IS NULL",
        };
        let rows = sqlx::query(&format!(
            "SELECT * FROM processed_email WHERE {condition}
             ORDER BY outcome = 'pending_review' DESC, processed_at DESC, message_id"
        ))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(processed_from_row).collect()
    }

    pub async fn unread_count(&self) -> Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT COUNT(*) FROM processed_email WHERE outcome != 'ignored' AND read_at IS NULL",
        )
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn mark_read(&self, message_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE processed_email SET read_at = CURRENT_TIMESTAMP WHERE message_id = ? AND read_at IS NULL",
        )
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_all_read(&self) -> Result<()> {
        sqlx::query("UPDATE processed_email SET read_at = CURRENT_TIMESTAMP WHERE read_at IS NULL")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Restores the status an auto-apply replaced, unless the job moved since.
    pub async fn undo_applied(&self, message_id: &str) -> Result<ProcessedEmail> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT * FROM processed_email WHERE message_id = ?")
            .bind(message_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let processed = processed_from_row(&row)?;
        let job_id = processed.matched_job_id.ok_or(StoreError::NotFound)?;
        let current: Option<Option<i64>> =
            sqlx::query_scalar("SELECT status_id FROM job_description WHERE id = ?")
                .bind(job_id)
                .fetch_optional(&mut *tx)
                .await?;
        let current = current.ok_or(StoreError::NotFound)?;
        if processed.outcome != Outcome::Applied
            || processed.undone_at.is_some()
            || current != processed.suggested_status_id
        {
            return Err(StoreError::Invalid(UNDO_BLOCKED.into()));
        }
        sqlx::query("UPDATE job_description SET status_id = ? WHERE id = ?")
            .bind(processed.previous_status_id)
            .bind(job_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "UPDATE processed_email SET undone_at = CURRENT_TIMESTAMP,
                read_at = COALESCE(read_at, CURRENT_TIMESTAMP) WHERE message_id = ?",
        )
        .bind(message_id)
        .execute(&mut *tx)
        .await?;
        let row = sqlx::query("SELECT * FROM processed_email WHERE message_id = ?")
            .bind(message_id)
            .fetch_one(&mut *tx)
            .await?;
        let result = processed_from_row(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    /// Recent ignored emails that looked job-related, or predate the score.
    pub async fn recheck_candidates(&self) -> Result<Vec<ProcessedEmail>> {
        let rows = sqlx::query(
            "SELECT * FROM processed_email
             WHERE outcome = 'ignored'
               AND processed_at >= datetime('now', '-30 days')
               AND (job_related >= 0.5 OR job_related IS NULL)
             ORDER BY processed_at, message_id",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(processed_from_row).collect()
    }

    pub async fn max_job_id(&self) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM job_description")
                .fetch_one(&self.pool)
                .await?,
        )
    }
}
