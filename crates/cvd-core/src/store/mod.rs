//! Backend-independent persistence API. SQLite is local-only; Postgres is tenant scoped.
mod email;
mod pg;
mod sqlite;
pub mod types;
pub use email::*;
use std::collections::BTreeMap;
use std::collections::HashSet;
use types::*;

/// Authenticated tenant identity. The HTTP layer must derive this from verified auth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UserId(pub uuid::Uuid);
impl UserId {
    /// Desktop and the local MCP server share this single identity.
    pub const LOCAL: Self = Self(uuid::Uuid::nil());
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not found")]
    NotFound,
    #[error("invalid stored value: {0}")]
    Invalid(String),
}
impl StoreError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound | Self::Db(sqlx::Error::RowNotFound))
    }
}
type Result<T> = std::result::Result<T, StoreError>;

#[derive(Clone)]
enum Backend {
    Sqlite(sqlite::Store),
    Postgres(pg::Store),
}
#[derive(Clone)]
pub struct Store {
    backend: Backend,
}
impl Store {
    pub async fn open(url: &str) -> Result<Self> {
        Ok(Self {
            backend: Backend::Sqlite(sqlite::Store::open(url).await?),
        })
    }
    /// SQLite only; never migrates an existing app database.
    pub async fn open_external(url: &str) -> Result<Self> {
        Ok(Self {
            backend: Backend::Sqlite(sqlite::Store::open_external(url).await?),
        })
    }
    /// Connect using a non-superuser role without BYPASSRLS. Migrate separately.
    pub async fn open_postgres(url: &str) -> Result<Self> {
        Ok(Self {
            backend: Backend::Postgres(pg::Store::open(url).await?),
        })
    }
    /// Run with the schema-owner connection before starting the server.
    pub async fn migrate_postgres(url: &str) -> Result<()> {
        pg::Store::migrate(url).await
    }
    /// Create the initial statuses and view during new-user provisioning.
    /// Call once after creating the auth user, before exposing their services.
    /// This is atomic, not an idempotent repair operation: duplicate defaults fail.
    /// SQLite already creates its local defaults through migrations.
    pub async fn create_user_defaults(&self, user: UserId) -> Result<()> {
        match &self.backend {
            Backend::Postgres(s) => s.create_user_defaults(user).await,
            Backend::Sqlite(_) if user == UserId::LOCAL => Ok(()),
            Backend::Sqlite(_) => Err(StoreError::Invalid(
                "SQLite only accepts the local user".into(),
            )),
        }
    }
    /// Bind a verified identity once when constructing request/application services.
    pub fn for_user(&self, user: UserId) -> TenantStore {
        TenantStore {
            store: self.clone(),
            user,
        }
    }
}
/// A store bound to one tenant. Shared services carry this instead of a raw pool.
#[derive(Clone)]
pub struct TenantStore {
    store: Store,
    user: UserId,
}
impl TenantStore {
    /// Desktop convenience constructor; always binds the fixed local user.
    pub async fn open(url: &str) -> Result<Self> {
        Ok(Store::open(url).await?.for_user(UserId::LOCAL))
    }
    pub async fn open_external(url: &str) -> Result<Self> {
        Ok(Store::open_external(url).await?.for_user(UserId::LOCAL))
    }
    pub fn user_id(&self) -> UserId {
        self.user
    }
    #[cfg(test)]
    pub(crate) async fn close(&self) {
        match &self.store.backend {
            Backend::Sqlite(s) => s.close().await,
            Backend::Postgres(_) => unreachable!("test helper only used by SQLite fault tests"),
        }
    }
}

// Keep the explicit-user API and the tenant-bound service adapter in sync.
macro_rules! operations {
    ($(fn $name:ident($($arg:ident: $ty:ty),*) -> $ret:ty;)*) => {
        impl Store {
            $(pub async fn $name(&self, user: UserId, $($arg: $ty),*) -> Result<$ret> {
                match &self.backend {
                    Backend::Sqlite(s) => {
                        if user != UserId::LOCAL {
                            return Err(StoreError::Invalid("SQLite only accepts the local user".into()));
                        }
                        s.$name($($arg),*).await
                    }
                    Backend::Postgres(s) => {
                        let mut tx = s.begin(user).await?;
                        let result = pg::$name(&mut tx, user, $($arg),*).await?;
                        tx.commit().await?;
                        Ok(result)
                    }
                }
            })*
        }
        impl TenantStore {
            $(pub async fn $name(&self, $($arg: $ty),*) -> Result<$ret> {
                self.store.$name(self.user, $($arg),*).await
            })*
        }
    };
}
operations! {
    fn get_profile() -> Option<Profile>;
    fn upsert_profile(p: &Profile) -> ();
    fn list_entries(include_archived: bool) -> Vec<Entry>;
    fn upsert_entry(e: &EntryInput) -> i64;
    fn find_entry_by_key(kind: EntryKind, title: &str, org: &str, start_date: &str) -> Option<i64>;
    fn delete_entry(id: i64) -> ();
    fn list_skills() -> Vec<Skill>;
    fn update_skill(id: i64, name: &str, category: &str) -> Option<Skill>;
    fn delete_skill(id: i64) -> bool;
    fn create_jd(title: &str, company: &str, raw_text: &str, url: &str, status_id: Option<i64>) -> i64;
    fn update_jd_tracking(id: i64, status_id: Option<i64>, url: &str) -> bool;
    fn update_jd_text(id: i64, raw_text: &str) -> bool;
    fn list_job_statuses() -> Vec<JobStatus>;
    fn find_job_status(name: &str) -> Option<JobStatus>;
    fn create_job_status(name: &str, color: StatusColor) -> i64;
    fn update_job_status(id: i64, name: &str, color: StatusColor) -> bool;
    fn delete_job_status(id: i64) -> ();
    fn reorder_job_statuses(ids: &[i64]) -> ();
    fn list_job_views() -> Vec<JobView>;
    fn get_job_view(id: i64) -> Option<JobView>;
    fn create_job_view(v: &JobViewInput) -> i64;
    fn update_job_view(id: i64, v: &JobViewInput) -> bool;
    fn delete_job_view(id: i64) -> ();
    fn get_jd(id: i64) -> Option<JobDescription>;
    fn list_jds() -> Vec<JobDescription>;
    fn create_run(jd_id: i64, model_id: &str) -> i64;
    fn get_run(run_id: i64) -> MatchRun;
    fn list_runs(jd_id: i64) -> Vec<MatchRun>;
    fn set_run_status(run_id: i64, status: RunStatus) -> ();
    fn insert_scores(rows: &[ScoreRow]) -> ();
    fn scores_for_run(run_id: i64) -> Vec<ScoreRow>;
    fn get_setting(key: &str) -> Option<String>;
    fn set_setting(key: &str, value: &str) -> ();
    fn delete_setting(key: &str) -> ();
    fn get_secret(provider: &str) -> Option<String>;
    fn set_secret(provider: &str, api_key: &str) -> ();
    fn delete_secret(provider: &str) -> ();
    fn save_cv(cv: &CvInput) -> i64;
    fn set_cv_file_ref(id: i64, file_ref: &str) -> ();
    fn delete_cv(id: i64) -> ();
    fn list_cvs(jd_id: i64) -> Vec<Cv>;
    fn change_versions() -> BTreeMap<String, i64>;
    fn find_company(domain: Option<&str>, normalized: &str) -> Option<Company>;
    fn create_company(name: &str, normalized: &str, domain: Option<&str>) -> Company;
    fn get_company(id: i64) -> Company;
    fn save_research(id: i64, canonical_name: &str, domain: Option<&str>, profile: &CompanyProfile, model_id: &str) -> Company;
    fn mark_research_failed(id: i64, error: &str, retry_after: &str) -> ();
    fn edit_company(id: i64, profile: &CompanyProfile) -> Company;
    fn add_alias(normalized: &str, company_id: i64) -> ();
    fn merge_company(from: i64, into: i64) -> ();
    fn set_jd_company(jd_id: i64, company_id: Option<i64>) -> ();
    fn set_run_company(run_id: i64, researched_at: Option<&str>) -> ();
    fn create_scan_run(source: ScanSource) -> i64;
    fn finish_scan_run(run_id: i64, status: ScanStatus, error: Option<&str>, checked: i64, failed: i64) -> ();
    fn get_scan_run(run_id: i64) -> Option<ScanRun>;
    fn processed_ids() -> HashSet<String>;
    fn record_processed(p: &NewProcessed, apply_status: Option<i64>) -> ();
    fn get_processed(message_id: &str) -> Option<ProcessedEmail>;
    fn list_run_results(run_id: i64) -> Vec<ProcessedEmail>;
    fn list_job_emails(job_id: i64) -> Vec<ProcessedEmail>;
    fn list_pending_reviews() -> Vec<ProcessedEmail>;
    fn apply_suggestion(message_id: &str) -> ProcessedEmail;
    fn dismiss_suggestion(message_id: &str) -> ();
    fn list_inbox(filter: InboxFilter) -> Vec<ProcessedEmail>;
    fn unread_count() -> i64;
    fn mark_read(message_id: &str) -> ();
    fn mark_all_read() -> ();
    fn undo_applied(message_id: &str) -> ProcessedEmail;
    fn recheck_candidates() -> Vec<ProcessedEmail>;
    fn max_job_id() -> i64;
    fn apply_memory() -> BTreeMap<String, String>;
    fn remember_label(label_norm: &str, fact_key: &str) -> ();
    fn forget_label(label_norm: &str) -> ();
}
#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "postgres-tests"))]
mod pg_security_tests;
