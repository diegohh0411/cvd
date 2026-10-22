use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Profile {
    pub name: String,
    pub headline: String,
    pub email: String,
    pub phone: String,
    pub location: String,
    pub links: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    Experience,
    Project,
    Education,
    Volunteer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Bullet {
    pub id: i64,
    pub entry_id: i64,
    pub text: String,
    pub position: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Skill {
    pub id: i64,
    pub name: String,
    pub category: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct JobDescription {
    pub id: i64,
    pub title: String,
    pub company: String,
    pub raw_text: String,
    pub created_at: String,
    /// Link to the original posting; empty when unknown.
    pub url: String,
    /// Application status (see `JobStatus`); null when unset.
    pub status_id: Option<i64>,
    /// Resolved company (see `Company`); null until a match resolves it.
    pub company_id: Option<i64>,
}

/// Notion's label palette; the UI maps each to light/dark styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum StatusColor {
    Gray,
    Brown,
    Orange,
    Yellow,
    Green,
    Blue,
    Purple,
    Pink,
    Red,
}

/// A user-editable application status label (Bookmarked, Applied, ...).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct JobStatus {
    pub id: i64,
    pub name: String,
    pub color: StatusColor,
    pub position: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewLayout {
    Table,
    Board,
}

/// A saved view of the Jobs page. `filters`/`sorts` are opaque JSON arrays the UI defines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobView {
    pub id: i64,
    pub name: String,
    pub layout: ViewLayout,
    pub filters: serde_json::Value,
    pub sorts: serde_json::Value,
    pub position: i64,
}

/// Fields of a view to create or save.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobViewInput {
    pub name: String,
    pub layout: ViewLayout,
    pub filters: serde_json::Value,
    pub sorts: serde_json::Value,
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
    pub created_at: String,
    /// `researched_at` of the company profile the run was scored with; null = no context.
    pub company_researched_at: Option<String>,
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
    pub file_ref: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CvInput {
    pub jd_id: i64,
    pub run_id: Option<i64>,
    pub template: String,
    pub page_budget: i64,
    pub selection_json: String,
    pub file_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub url: String,
    pub title: String,
}

/// Researched (or hand-edited) facts about a company.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanyProfile {
    pub summary: String,
    pub industry: String,
    pub size: String,
    pub stage: String,
    pub headquarters: String,
    pub remote_policy: String,
    pub products: String,
    pub tech_stack: Vec<String>,
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompanyStatus {
    Pending,
    Ok,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Company {
    pub id: i64,
    pub name: String,
    pub normalized_name: String,
    pub domain: Option<String>,
    pub profile: CompanyProfile,
    pub status: CompanyStatus,
    pub researched_at: Option<String>,
    pub model_id: String,
    pub fail_count: i64,
    pub last_error: String,
    pub retry_after: Option<String>,
    pub edited_at: Option<String>,
}
