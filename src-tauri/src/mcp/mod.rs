//! MCP server exposing the profile store to local agents (Claude Code, OpenCode).

mod validate;

#[cfg(test)]
mod tests;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::{tool, tool_handler, tool_router, ErrorData, Json, ServerHandler};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::store::types::{Entry, EntryInput, JobDescription, JobStatus, Profile, Skill, StatusColor};
use crate::store::{Store, StoreError};

pub use validate::normalize_entry;

/// Tauri's bundle identifier; `app_data_dir()` is `<data_dir>/<identifier>`.
const APP_IDENTIFIER: &str = "com.diego.cv-decisions";

/// Resolves the app database: `override_path` (from `CVD_DB_PATH`) or the
/// same `cv.db` the desktop app opens.
pub fn db_path_from(override_path: Option<std::path::PathBuf>) -> Option<std::path::PathBuf> {
    override_path.or_else(|| dirs::data_dir().map(|d| d.join(APP_IDENTIFIER).join("cv.db")))
}

const INSTRUCTIONS: &str = "Stores the user's CV profile: experience, projects, education and volunteer entries. \
Typical workflow: read the source (e.g. Notion pages), call list_entries and list_skills first, \
then call upsert_entry once per entry, reusing existing skill names. Dates are YYYY-MM; \
omit end_date for ongoing roles. To change an existing entry pass its id. Use dry_run to preview. \
It also tracks job applications: list_jobs shows saved postings with their status and URL; \
upsert_job saves a new posting or moves an existing one to another status (names from list_job_statuses); \
upsert_job_status adds or edits the status labels themselves.";

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListEntriesArgs {
    /// Include archived entries.
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpsertEntryArgs {
    /// The entry. `id: null` inserts, `id: <n>` replaces entry n (bullets and skills included).
    pub entry: EntryInput,
    /// Validate and return the normalized entry without writing.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpsertProfileArgs {
    pub profile: Profile,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UpsertEntryResult {
    /// Saved entry id; null on dry runs.
    pub id: Option<i64>,
    /// The entry as normalized (trimmed, blanks dropped).
    pub entry: EntryInput,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct EntryList {
    pub entries: Vec<Entry>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SkillList {
    pub skills: Vec<Skill>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ProfileResult {
    pub profile: Option<Profile>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpsertJobArgs {
    /// Existing job id to update; omit to create a new job.
    #[serde(default)]
    pub id: Option<i64>,
    /// Role title. Required when creating.
    #[serde(default)]
    pub title: Option<String>,
    /// Company name. Used when creating.
    #[serde(default)]
    pub company: Option<String>,
    /// Full job description text. Required when creating.
    #[serde(default)]
    pub description: Option<String>,
    /// Link to the posting. Omit to keep the current one; "" clears it.
    #[serde(default)]
    pub url: Option<String>,
    /// Status name from list_job_statuses (case-insensitive). Omit to keep the
    /// current status (or use the first status when creating); "" clears it.
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpsertJobStatusArgs {
    /// Existing status id to rename/recolor; omit to create one.
    #[serde(default)]
    pub id: Option<i64>,
    pub name: String,
    pub color: StatusColor,
}

/// A job posting with its status name resolved.
#[derive(Debug, Serialize, JsonSchema)]
pub struct JobSummary {
    pub id: i64,
    pub title: String,
    pub company: String,
    pub url: String,
    pub status: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct JobList {
    pub jobs: Vec<JobSummary>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct JobStatusList {
    pub statuses: Vec<JobStatus>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct UpsertJobResult {
    pub job: JobSummary,
}

fn summarize(j: JobDescription, statuses: &[JobStatus]) -> JobSummary {
    JobSummary {
        status: j.status_id.and_then(|id| statuses.iter().find(|s| s.id == id)).map(|s| s.name.clone()),
        id: j.id,
        title: j.title,
        company: j.company,
        url: j.url,
        created_at: j.created_at,
    }
}

fn internal(e: StoreError) -> ErrorData {
    match e {
        StoreError::Invalid(msg) => ErrorData::invalid_params(msg, None),
        other => ErrorData::internal_error(other.to_string(), None),
    }
}

#[derive(Clone)]
pub struct CvServer {
    store: Store,
    tool_router: ToolRouter<Self>,
}

/// Plain async API, used by the tools below and by tests.
impl CvServer {
    pub async fn list_entries(&self, include_archived: bool) -> Result<Vec<Entry>, ErrorData> {
        self.store.list_entries(include_archived).await.map_err(internal)
    }

    pub async fn upsert_entry(&self, entry: EntryInput, dry_run: bool) -> Result<UpsertEntryResult, ErrorData> {
        let entry = normalize_entry(entry).map_err(|m| ErrorData::invalid_params(m, None))?;
        if entry.id.is_none() {
            let existing = self
                .store
                .find_entry_by_key(entry.kind, &entry.title, &entry.org, &entry.start_date)
                .await
                .map_err(internal)?;
            if let Some(id) = existing {
                return Err(ErrorData::invalid_params(
                    format!("entry already exists as id {id}; pass id to update it"),
                    None,
                ));
            }
        }
        if dry_run {
            return Ok(UpsertEntryResult { id: None, entry });
        }
        let id = self.store.upsert_entry(&entry).await.map_err(internal)?;
        Ok(UpsertEntryResult { id: Some(id), entry })
    }

    pub async fn list_jobs(&self) -> Result<Vec<JobSummary>, ErrorData> {
        let statuses = self.store.list_job_statuses().await.map_err(internal)?;
        let jobs = self.store.list_jds().await.map_err(internal)?;
        Ok(jobs.into_iter().map(|j| summarize(j, &statuses)).collect())
    }

    /// `Some("")` clears; unknown names list the valid ones.
    async fn resolve_status(&self, name: &str) -> Result<Option<i64>, ErrorData> {
        if name.trim().is_empty() {
            return Ok(None);
        }
        match self.store.find_job_status(name).await.map_err(internal)? {
            Some(s) => Ok(Some(s.id)),
            None => {
                let names: Vec<_> = self
                    .store
                    .list_job_statuses()
                    .await
                    .map_err(internal)?
                    .into_iter()
                    .map(|s| s.name)
                    .collect();
                Err(ErrorData::invalid_params(
                    format!("unknown status \"{}\"; use one of: {}", name.trim(), names.join(", ")),
                    None,
                ))
            }
        }
    }

    pub async fn upsert_job(&self, a: UpsertJobArgs) -> Result<JobSummary, ErrorData> {
        let status_id = match &a.status {
            Some(name) => Some(self.resolve_status(name).await?),
            None => None,
        };
        let id = match a.id {
            Some(id) => {
                let cur = self
                    .store
                    .get_jd(id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| ErrorData::invalid_params(format!("no job with id {id}"), None))?;
                if a.title.is_some() || a.company.is_some() || a.description.is_some() {
                    return Err(ErrorData::invalid_params(
                        "title, company and description cannot change on an existing job; only status and url",
                        None,
                    ));
                }
                let url = a.url.as_deref().unwrap_or(&cur.url);
                let status_id = status_id.unwrap_or(cur.status_id);
                self.store.update_jd_tracking(id, status_id, url).await.map_err(internal)?;
                id
            }
            None => {
                let title = a.title.as_deref().map(str::trim).unwrap_or_default();
                let text = a.description.as_deref().map(str::trim).unwrap_or_default();
                if title.is_empty() || text.is_empty() {
                    return Err(ErrorData::invalid_params("title and description are required to create a job", None));
                }
                let company = a.company.as_deref().map(str::trim).unwrap_or_default();
                let url = a.url.as_deref().unwrap_or_default();
                let id = self
                    .store
                    .create_jd(title, company, text, url, status_id.flatten())
                    .await
                    .map_err(internal)?;
                if status_id == Some(None) {
                    // An explicit "" status means none, not the default.
                    self.store.update_jd_tracking(id, None, url).await.map_err(internal)?;
                }
                id
            }
        };
        let statuses = self.store.list_job_statuses().await.map_err(internal)?;
        let job = self.store.get_jd(id).await.map_err(internal)?.ok_or_else(|| {
            ErrorData::internal_error(format!("job {id} vanished"), None)
        })?;
        Ok(summarize(job, &statuses))
    }

    pub async fn upsert_job_status(&self, a: UpsertJobStatusArgs) -> Result<JobStatus, ErrorData> {
        let id = match a.id {
            Some(id) => {
                if !self.store.update_job_status(id, &a.name, a.color).await.map_err(internal)? {
                    return Err(ErrorData::invalid_params(format!("no status with id {id}"), None));
                }
                id
            }
            None => self.store.create_job_status(&a.name, a.color).await.map_err(internal)?,
        };
        self.store
            .list_job_statuses()
            .await
            .map_err(internal)?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| ErrorData::internal_error(format!("status {id} vanished"), None))
    }
}

#[tool_router(router = tool_router)]
impl CvServer {
    pub fn new(store: Store) -> Self {
        Self { store, tool_router: Self::tool_router() }
    }

    /// List all CV entries with their bullets and skills. Call before inserting to avoid duplicates.
    #[tool(name = "list_entries")]
    async fn tool_list_entries(&self, Parameters(a): Parameters<ListEntriesArgs>) -> Result<Json<EntryList>, ErrorData> {
        Ok(Json(EntryList { entries: self.list_entries(a.include_archived).await? }))
    }

    /// List known skills. Reuse these names instead of inventing variants.
    #[tool(name = "list_skills")]
    async fn tool_list_skills(&self) -> Result<Json<SkillList>, ErrorData> {
        Ok(Json(SkillList { skills: self.store.list_skills().await.map_err(internal)? }))
    }

    /// Get the profile header (name, headline, contact, summary).
    #[tool(name = "get_profile")]
    async fn tool_get_profile(&self) -> Result<Json<ProfileResult>, ErrorData> {
        Ok(Json(ProfileResult { profile: self.store.get_profile().await.map_err(internal)? }))
    }

    /// Replace the profile header.
    #[tool(name = "upsert_profile")]
    async fn tool_upsert_profile(&self, Parameters(a): Parameters<UpsertProfileArgs>) -> Result<String, ErrorData> {
        self.store.upsert_profile(&a.profile).await.map_err(internal)?;
        Ok("ok".into())
    }

    /// Insert or update one CV entry (experience, project, education, volunteer).
    /// Call list_entries and list_skills first. Dates are YYYY-MM.
    #[tool(name = "upsert_entry")]
    async fn tool_upsert_entry(&self, Parameters(a): Parameters<UpsertEntryArgs>) -> Result<Json<UpsertEntryResult>, ErrorData> {
        Ok(Json(self.upsert_entry(a.entry, a.dry_run).await?))
    }

    /// List saved job postings with their application status and URL, newest first.
    #[tool(name = "list_jobs")]
    async fn tool_list_jobs(&self) -> Result<Json<JobList>, ErrorData> {
        Ok(Json(JobList { jobs: self.list_jobs().await? }))
    }

    /// List the application status labels (e.g. Bookmarked, Applied, Interviewing, Offer) in board order.
    #[tool(name = "list_job_statuses")]
    async fn tool_list_job_statuses(&self) -> Result<Json<JobStatusList>, ErrorData> {
        Ok(Json(JobStatusList { statuses: self.store.list_job_statuses().await.map_err(internal)? }))
    }

    /// Save a new job posting (title + description required), or update an existing
    /// job's status and/or URL by id. Status is a name from list_job_statuses.
    #[tool(name = "upsert_job")]
    async fn tool_upsert_job(&self, Parameters(a): Parameters<UpsertJobArgs>) -> Result<Json<UpsertJobResult>, ErrorData> {
        Ok(Json(UpsertJobResult { job: self.upsert_job(a).await? }))
    }

    /// Create a status label, or rename/recolor one by id. Colors: gray, brown,
    /// orange, yellow, green, blue, purple, pink, red.
    #[tool(name = "upsert_job_status")]
    async fn tool_upsert_job_status(&self, Parameters(a): Parameters<UpsertJobStatusArgs>) -> Result<Json<JobStatus>, ErrorData> {
        Ok(Json(self.upsert_job_status(a).await?))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CvServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("cvd-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}
