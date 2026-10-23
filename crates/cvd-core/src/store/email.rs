//! Shared mail persistence contracts.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ignored,
    NoChange,
    Applied,
    PendingReview,
    Dismissed,
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
