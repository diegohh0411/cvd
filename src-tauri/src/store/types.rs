use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub headline: String,
    pub email: String,
    pub phone: String,
    pub location: String,
    pub links: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Experience,
    Project,
    Education,
    Volunteer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bullet {
    pub id: i64,
    pub entry_id: i64,
    pub text: String,
    pub position: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Skill {
    pub id: i64,
    pub name: String,
    pub category: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: i64,
    pub kind: EntryKind,
    pub title: String,
    pub org: String,
    pub location: String,
    /// `YYYY-MM`
    pub start_date: String,
    pub end_date: Option<String>,
    pub description: String,
    pub sort_hint: i64,
    pub archived: bool,
    pub bullets: Vec<Bullet>,
    pub skills: Vec<Skill>,
}

/// Writable fields of an entry. `id: Some(_)` updates, `None` inserts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryInput {
    pub id: Option<i64>,
    pub kind: EntryKind,
    pub title: String,
    pub org: String,
    pub location: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub description: String,
    pub sort_hint: i64,
    pub archived: bool,
    /// Bullet texts in display order.
    pub bullets: Vec<String>,
    /// Skill names; created on demand (case-insensitive).
    pub skills: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobDescription {
    pub id: i64,
    pub title: String,
    pub company: String,
    pub raw_text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Done,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchRun {
    pub id: i64,
    pub jd_id: i64,
    pub model_id: String,
    pub status: RunStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetType {
    Entry,
    Bullet,
    Skill,
    Meta,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreRow {
    pub run_id: i64,
    pub target_type: TargetType,
    pub target_id: i64,
    pub question: String,
    pub probabilities: BTreeMap<String, f64>,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cv {
    pub id: i64,
    pub jd_id: i64,
    pub run_id: Option<i64>,
    pub template: String,
    pub page_budget: i64,
    pub selection_json: String,
    pub pdf_path: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CvInput {
    pub jd_id: i64,
    pub run_id: Option<i64>,
    pub template: String,
    pub page_budget: i64,
    pub selection_json: String,
    pub pdf_path: Option<String>,
}
