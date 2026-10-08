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

use crate::store::types::{Entry, EntryInput, Profile, Skill};
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
omit end_date for ongoing roles. To change an existing entry pass its id. Use dry_run to preview.";

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
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CvServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("cvd-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}
