mod apply_memory;
mod companies;
mod email;
pub mod types;
pub use email::{
    InboxFilter, NewProcessed, Outcome, ProcessedEmail, ScanRun, ScanSource, ScanStatus,
    UNDO_BLOCKED,
};

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};

use types::*;

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

type Result<T> = std::result::Result<T, StoreError>;

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

fn kind_str(k: EntryKind) -> &'static str {
    match k {
        EntryKind::Experience => "experience",
        EntryKind::Project => "project",
        EntryKind::Education => "education",
        EntryKind::Volunteer => "volunteer",
    }
}

fn parse_kind(s: &str) -> Result<EntryKind> {
    Ok(match s {
        "experience" => EntryKind::Experience,
        "project" => EntryKind::Project,
        "education" => EntryKind::Education,
        "volunteer" => EntryKind::Volunteer,
        other => return Err(StoreError::Invalid(format!("entry kind {other}"))),
    })
}

fn status_str(s: RunStatus) -> &'static str {
    match s {
        RunStatus::Running => "running",
        RunStatus::Done => "done",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Failed => "failed",
    }
}

fn parse_status(s: &str) -> Result<RunStatus> {
    Ok(match s {
        "running" => RunStatus::Running,
        "done" => RunStatus::Done,
        "cancelled" => RunStatus::Cancelled,
        "failed" => RunStatus::Failed,
        other => return Err(StoreError::Invalid(format!("run status {other}"))),
    })
}

fn target_str(t: TargetType) -> &'static str {
    match t {
        TargetType::Entry => "entry",
        TargetType::Bullet => "bullet",
        TargetType::Skill => "skill",
        TargetType::Meta => "meta",
    }
}

fn parse_target(s: &str) -> Result<TargetType> {
    Ok(match s {
        "entry" => TargetType::Entry,
        "bullet" => TargetType::Bullet,
        "skill" => TargetType::Skill,
        "meta" => TargetType::Meta,
        other => return Err(StoreError::Invalid(format!("target type {other}"))),
    })
}

fn color_str(c: StatusColor) -> &'static str {
    match c {
        StatusColor::Gray => "gray",
        StatusColor::Brown => "brown",
        StatusColor::Orange => "orange",
        StatusColor::Yellow => "yellow",
        StatusColor::Green => "green",
        StatusColor::Blue => "blue",
        StatusColor::Purple => "purple",
        StatusColor::Pink => "pink",
        StatusColor::Red => "red",
    }
}

fn parse_color(s: &str) -> Result<StatusColor> {
    serde_json::from_value(serde_json::Value::String(s.into()))
        .map_err(|_| StoreError::Invalid(format!("status color {s}")))
}

fn job_status_from_row(r: &SqliteRow) -> Result<JobStatus> {
    Ok(JobStatus {
        id: r.get("id"),
        name: r.get("name"),
        color: parse_color(r.get("color"))?,
        position: r.get("position"),
    })
}

/// Trims a status name, rejecting blanks.
fn status_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::Invalid("status name must not be empty".into()));
    }
    Ok(name)
}

/// Turns a UNIQUE violation on `job_status.name` into a readable error.
fn dup_status(e: sqlx::Error, name: &str) -> StoreError {
    match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => {
            StoreError::Invalid(format!("a status named \"{name}\" already exists"))
        }
        _ => StoreError::Db(e),
    }
}

fn job_view_from_row(r: &SqliteRow) -> Result<JobView> {
    let layout: String = r.get("layout");
    Ok(JobView {
        id: r.get("id"),
        name: r.get("name"),
        layout: serde_json::from_value(serde_json::Value::String(layout.clone()))
            .map_err(|_| StoreError::Invalid(format!("view layout {layout}")))?,
        filters: serde_json::from_str(r.get("filters"))?,
        sorts: serde_json::from_str(r.get("sorts"))?,
        position: r.get("position"),
    })
}

/// Validates a view's fields, returning (name, layout, filters, sorts) ready to bind.
fn view_columns(v: &JobViewInput) -> Result<(&str, &'static str, String, String)> {
    let name = v.name.trim();
    if name.is_empty() {
        return Err(StoreError::Invalid("view name must not be empty".into()));
    }
    if !v.filters.is_array() || !v.sorts.is_array() {
        return Err(StoreError::Invalid(
            "view filters and sorts must be lists".into(),
        ));
    }
    let layout = match v.layout {
        ViewLayout::Table => "table",
        ViewLayout::Board => "board",
    };
    Ok((name, layout, v.filters.to_string(), v.sorts.to_string()))
}

fn skill_from_row(r: &SqliteRow) -> Skill {
    Skill {
        id: r.get("id"),
        name: r.get("name"),
        category: r.get("category"),
    }
}

fn jd_from_row(r: &SqliteRow) -> JobDescription {
    JobDescription {
        id: r.get("id"),
        title: r.get("title"),
        company: r.get("company"),
        raw_text: r.get("raw_text"),
        created_at: r.get("created_at"),
        url: r.get("url"),
        status_id: r.get("status_id"),
        company_id: r.get("company_id"),
    }
}

fn run_from_row(r: &SqliteRow) -> Result<MatchRun> {
    Ok(MatchRun {
        id: r.get("id"),
        jd_id: r.get("jd_id"),
        model_id: r.get("model_id"),
        status: parse_status(r.get("status"))?,
        created_at: r.get("created_at"),
        company_researched_at: r.get("company_researched_at"),
    })
}

fn cv_from_row(r: &SqliteRow) -> Cv {
    Cv {
        id: r.get("id"),
        jd_id: r.get("jd_id"),
        run_id: r.get("run_id"),
        template: r.get("template"),
        page_budget: r.get("page_budget"),
        selection_json: r.get("selection_json"),
        pdf_path: r.get("pdf_path"),
        created_at: r.get("created_at"),
    }
}

impl Store {
    async fn connect(url: &str) -> Result<SqlitePool> {
        let mut opts = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true)
            .foreign_keys(true);
        let mut pool_opts = SqlitePoolOptions::new();
        if url.contains(":memory:") {
            pool_opts = pool_opts.max_connections(1);
        } else {
            // WAL + busy timeout let the app and the MCP server write concurrently.
            opts = opts
                .journal_mode(SqliteJournalMode::Wal)
                .busy_timeout(Duration::from_secs(5));
        }
        let pool = pool_opts.connect_with(opts).await?;
        Ok(pool)
    }

    /// Opens (creating if needed) the database at `url` and runs migrations.
    /// In-memory URLs use a single connection so the database is shared.
    pub async fn open(url: &str) -> Result<Store> {
        let pool = Self::connect(url).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Store { pool })
    }

    /// Opens the database for a process other than the app (the MCP server).
    /// An existing database is used as-is, never migrated, so an older or
    /// newer build of this binary can't conflict with the app's schema.
    pub async fn open_external(url: &str) -> Result<Store> {
        let pool = Self::connect(url).await?;
        let migrated: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations')",
        )
        .fetch_one(&pool)
        .await?;
        if !migrated {
            sqlx::migrate!("./migrations").run(&pool).await?;
        }
        Ok(Store { pool })
    }

    // ---- profile ----

    pub async fn get_profile(&self) -> Result<Option<Profile>> {
        let row = sqlx::query("SELECT * FROM profile WHERE id = 1")
            .fetch_optional(&self.pool)
            .await?;
        row.map(|r| {
            Ok(Profile {
                name: r.get("name"),
                headline: r.get("headline"),
                email: r.get("email"),
                phone: r.get("phone"),
                location: r.get("location"),
                links: serde_json::from_str(r.get::<&str, _>("links_json"))?,
                summary: r.get("summary"),
            })
        })
        .transpose()
    }

    pub async fn upsert_profile(&self, p: &Profile) -> Result<()> {
        sqlx::query(
            "INSERT INTO profile (id, name, headline, email, phone, location, links_json, summary)
             VALUES (1, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, headline = excluded.headline,
               email = excluded.email, phone = excluded.phone, location = excluded.location,
               links_json = excluded.links_json, summary = excluded.summary",
        )
        .bind(&p.name)
        .bind(&p.headline)
        .bind(&p.email)
        .bind(&p.phone)
        .bind(&p.location)
        .bind(serde_json::to_string(&p.links)?)
        .bind(&p.summary)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ---- entries ----

    pub async fn list_entries(&self, include_archived: bool) -> Result<Vec<Entry>> {
        let rows = sqlx::query(
            "SELECT * FROM entry WHERE (? OR archived = 0) ORDER BY start_date DESC, id",
        )
        .bind(include_archived)
        .fetch_all(&self.pool)
        .await?;
        let mut entries = Vec::with_capacity(rows.len());
        for r in rows {
            let id: i64 = r.get("id");
            let bullets = sqlx::query(
                "SELECT id, entry_id, text, position FROM bullet WHERE entry_id = ? ORDER BY position, id",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(|b| Bullet {
                id: b.get("id"),
                entry_id: b.get("entry_id"),
                text: b.get("text"),
                position: b.get("position"),
            })
            .collect();
            let skills = sqlx::query(
                "SELECT s.id, s.name, s.category FROM skill s
                 JOIN entry_skill es ON es.skill_id = s.id
                 WHERE es.entry_id = ? ORDER BY s.name",
            )
            .bind(id)
            .fetch_all(&self.pool)
            .await?
            .iter()
            .map(skill_from_row)
            .collect();
            entries.push(Entry {
                id,
                kind: parse_kind(r.get("kind"))?,
                title: r.get("title"),
                org: r.get("org"),
                location: r.get("location"),
                start_date: r.get("start_date"),
                end_date: r.get("end_date"),
                description: r.get("description"),
                sort_hint: r.get("sort_hint"),
                archived: r.get("archived"),
                bullets,
                skills,
            });
        }
        Ok(entries)
    }

    /// Inserts or updates an entry, replacing its bullets and skill links in
    /// one transaction. Unknown skill names are created.
    pub async fn upsert_entry(&self, e: &EntryInput) -> Result<i64> {
        let mut tx = self.pool.begin().await?;
        let id = match e.id {
            Some(id) => {
                let res = sqlx::query(
                    "UPDATE entry SET kind = ?, title = ?, org = ?, location = ?, start_date = ?,
                       end_date = ?, description = ?, sort_hint = ?, archived = ? WHERE id = ?",
                )
                .bind(kind_str(e.kind))
                .bind(&e.title)
                .bind(&e.org)
                .bind(&e.location)
                .bind(&e.start_date)
                .bind(&e.end_date)
                .bind(&e.description)
                .bind(e.sort_hint)
                .bind(e.archived)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                if res.rows_affected() == 0 {
                    return Err(StoreError::Invalid(format!("no entry with id {id}")));
                }
                sqlx::query("DELETE FROM bullet WHERE entry_id = ?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("DELETE FROM entry_skill WHERE entry_id = ?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                id
            }
            None => sqlx::query(
                "INSERT INTO entry (kind, title, org, location, start_date, end_date, description, sort_hint, archived)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(kind_str(e.kind))
            .bind(&e.title)
            .bind(&e.org)
            .bind(&e.location)
            .bind(&e.start_date)
            .bind(&e.end_date)
            .bind(&e.description)
            .bind(e.sort_hint)
            .bind(e.archived)
            .execute(&mut *tx)
            .await?
            .last_insert_rowid(),
        };
        for (pos, text) in e.bullets.iter().enumerate() {
            sqlx::query("INSERT INTO bullet (entry_id, text, position) VALUES (?, ?, ?)")
                .bind(id)
                .bind(text)
                .bind(pos as i64)
                .execute(&mut *tx)
                .await?;
        }
        for name in &e.skills {
            sqlx::query("INSERT OR IGNORE INTO skill (name) VALUES (?)")
                .bind(name)
                .execute(&mut *tx)
                .await?;
            // skill.name is COLLATE NOCASE, so this matches case-insensitively.
            sqlx::query(
                "INSERT OR IGNORE INTO entry_skill (entry_id, skill_id)
                 SELECT ?, id FROM skill WHERE name = ?",
            )
            .bind(id)
            .bind(name)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(id)
    }

    /// Finds an entry by its natural key; title and org match case-insensitively.
    pub async fn find_entry_by_key(
        &self,
        kind: EntryKind,
        title: &str,
        org: &str,
        start_date: &str,
    ) -> Result<Option<i64>> {
        let id = sqlx::query_scalar(
            "SELECT id FROM entry WHERE kind = ? AND title = ? COLLATE NOCASE
               AND org = ? COLLATE NOCASE AND start_date = ? LIMIT 1",
        )
        .bind(kind_str(kind))
        .bind(title)
        .bind(org)
        .bind(start_date)
        .fetch_optional(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn delete_entry(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM entry WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_skills(&self) -> Result<Vec<Skill>> {
        let rows = sqlx::query("SELECT id, name, category FROM skill ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(skill_from_row).collect())
    }

    /// Renames and recategorizes a skill. Renaming onto another skill's name
    /// (case-insensitive) merges into it: entries move over and `id` is
    /// deleted. Returns the surviving skill, or None when `id` does not exist.
    pub async fn update_skill(&self, id: i64, name: &str, category: &str) -> Result<Option<Skill>> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StoreError::Invalid("skill name must not be empty".into()));
        }
        let mut tx = self.pool.begin().await?;
        if sqlx::query("SELECT 1 FROM skill WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .is_none()
        {
            return Ok(None);
        }
        let other: Option<i64> =
            sqlx::query_scalar("SELECT id FROM skill WHERE name = ? AND id != ?")
                .bind(name)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        let target = match other {
            Some(target) => {
                sqlx::query(
                    "INSERT OR IGNORE INTO entry_skill (entry_id, skill_id)
                     SELECT entry_id, ? FROM entry_skill WHERE skill_id = ?",
                )
                .bind(target)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                sqlx::query("DELETE FROM skill WHERE id = ?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await?;
                target
            }
            None => id,
        };
        sqlx::query("UPDATE skill SET name = ?, category = ? WHERE id = ?")
            .bind(name)
            .bind(category.trim())
            .bind(target)
            .execute(&mut *tx)
            .await?;
        let row = sqlx::query("SELECT id, name, category FROM skill WHERE id = ?")
            .bind(target)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(skill_from_row(&row)))
    }

    /// Deletes a skill and unlinks it from every entry. Returns false when it does not exist.
    pub async fn delete_skill(&self, id: i64) -> Result<bool> {
        let done = sqlx::query("DELETE FROM skill WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    // ---- job descriptions ----

    /// Inserts a job. `status_id: None` picks the first status (e.g. Bookmarked).
    pub async fn create_jd(
        &self,
        title: &str,
        company: &str,
        raw_text: &str,
        url: &str,
        status_id: Option<i64>,
    ) -> Result<i64> {
        let status_id = match status_id {
            Some(id) => Some(id),
            None => {
                sqlx::query_scalar("SELECT id FROM job_status ORDER BY position, id LIMIT 1")
                    .fetch_optional(&self.pool)
                    .await?
            }
        };
        Ok(sqlx::query(
            "INSERT INTO job_description (title, company, raw_text, url, status_id) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(title)
        .bind(company)
        .bind(raw_text)
        .bind(url.trim())
        .bind(status_id)
        .execute(&self.pool)
        .await?
        .last_insert_rowid())
    }

    /// Sets a job's tracking fields; `status_id: None` clears the status.
    /// Returns false when the job does not exist.
    pub async fn update_jd_tracking(
        &self,
        id: i64,
        status_id: Option<i64>,
        url: &str,
    ) -> Result<bool> {
        let done = sqlx::query("UPDATE job_description SET status_id = ?, url = ? WHERE id = ?")
            .bind(status_id)
            .bind(url.trim())
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Replaces a job's description text. Returns false when the job does not exist.
    pub async fn update_jd_text(&self, id: i64, raw_text: &str) -> Result<bool> {
        let done = sqlx::query("UPDATE job_description SET raw_text = ? WHERE id = ?")
            .bind(raw_text)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() > 0)
    }

    // ---- job statuses ----

    pub async fn list_job_statuses(&self) -> Result<Vec<JobStatus>> {
        let rows = sqlx::query("SELECT * FROM job_status ORDER BY position, id")
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(job_status_from_row).collect()
    }

    /// Case-insensitive lookup by name.
    pub async fn find_job_status(&self, name: &str) -> Result<Option<JobStatus>> {
        let row = sqlx::query("SELECT * FROM job_status WHERE name = ?")
            .bind(name.trim())
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(job_status_from_row).transpose()
    }

    /// Appends a status at the end of the list.
    pub async fn create_job_status(&self, name: &str, color: StatusColor) -> Result<i64> {
        let name = status_name(name)?;
        Ok(sqlx::query(
            "INSERT INTO job_status (name, color, position)
             VALUES (?, ?, (SELECT COALESCE(MAX(position) + 1, 0) FROM job_status))",
        )
        .bind(name)
        .bind(color_str(color))
        .execute(&self.pool)
        .await
        .map_err(|e| dup_status(e, name))?
        .last_insert_rowid())
    }

    /// Renames/recolors a status. Returns false when it does not exist.
    pub async fn update_job_status(&self, id: i64, name: &str, color: StatusColor) -> Result<bool> {
        let name = status_name(name)?;
        let done = sqlx::query("UPDATE job_status SET name = ?, color = ? WHERE id = ?")
            .bind(name)
            .bind(color_str(color))
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| dup_status(e, name))?;
        Ok(done.rows_affected() > 0)
    }

    /// Deletes a status; jobs using it lose their status.
    pub async fn delete_job_status(&self, id: i64) -> Result<()> {
        sqlx::query("DELETE FROM job_status WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Sets positions to the order of `ids`; ids not listed keep theirs.
    pub async fn reorder_job_statuses(&self, ids: &[i64]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for (pos, id) in ids.iter().enumerate() {
            sqlx::query("UPDATE job_status SET position = ? WHERE id = ?")
                .bind(pos as i64)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    // ---- job views ----

    pub async fn list_job_views(&self) -> Result<Vec<JobView>> {
        let rows = sqlx::query("SELECT * FROM job_view ORDER BY position, id")
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(job_view_from_row).collect()
    }

    pub async fn get_job_view(&self, id: i64) -> Result<Option<JobView>> {
        let row = sqlx::query("SELECT * FROM job_view WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(job_view_from_row).transpose()
    }

    /// Appends a view at the end of the tabs.
    pub async fn create_job_view(&self, v: &JobViewInput) -> Result<i64> {
        let (name, layout, filters, sorts) = view_columns(v)?;
        Ok(sqlx::query(
            "INSERT INTO job_view (name, layout, filters, sorts, position)
             VALUES (?, ?, ?, ?, (SELECT COALESCE(MAX(position) + 1, 0) FROM job_view))",
        )
        .bind(name)
        .bind(layout)
        .bind(filters)
        .bind(sorts)
        .execute(&self.pool)
        .await?
        .last_insert_rowid())
    }

    /// Saves a view. Returns false when it does not exist.
    pub async fn update_job_view(&self, id: i64, v: &JobViewInput) -> Result<bool> {
        let (name, layout, filters, sorts) = view_columns(v)?;
        let done = sqlx::query(
            "UPDATE job_view SET name = ?, layout = ?, filters = ?, sorts = ? WHERE id = ?",
        )
        .bind(name)
        .bind(layout)
        .bind(filters)
        .bind(sorts)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Deletes a view, refusing to remove the last one so the page always has a view.
    pub async fn delete_job_view(&self, id: i64) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let others: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM job_view WHERE id != ?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if others == 0 {
            return Err(StoreError::Invalid(
                "the last view cannot be deleted".into(),
            ));
        }
        sqlx::query("DELETE FROM job_view WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_jd(&self, id: i64) -> Result<Option<JobDescription>> {
        let row = sqlx::query("SELECT * FROM job_description WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.as_ref().map(jd_from_row))
    }

    pub async fn list_jds(&self) -> Result<Vec<JobDescription>> {
        let rows = sqlx::query("SELECT * FROM job_description ORDER BY created_at DESC, id DESC")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(jd_from_row).collect())
    }

    // ---- match runs ----

    /// Creates a run in `running` status.
    pub async fn create_run(&self, jd_id: i64, model_id: &str) -> Result<i64> {
        Ok(
            sqlx::query("INSERT INTO match_run (jd_id, model_id, status) VALUES (?, ?, 'running')")
                .bind(jd_id)
                .bind(model_id)
                .execute(&self.pool)
                .await?
                .last_insert_rowid(),
        )
    }

    /// Fetches a run; errors with `RowNotFound` if it does not exist.
    pub async fn get_run(&self, run_id: i64) -> Result<MatchRun> {
        let r = sqlx::query("SELECT * FROM match_run WHERE id = ?")
            .bind(run_id)
            .fetch_one(&self.pool)
            .await?;
        run_from_row(&r)
    }

    /// Runs for a job, newest first.
    pub async fn list_runs(&self, jd_id: i64) -> Result<Vec<MatchRun>> {
        let rows = sqlx::query("SELECT * FROM match_run WHERE jd_id = ? ORDER BY id DESC")
            .bind(jd_id)
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(run_from_row).collect()
    }

    #[cfg(test)]
    pub(crate) async fn close(&self) {
        self.pool.close().await;
    }

    /// Records the model a run is (re)scored with.
    pub async fn set_run_model(&self, run_id: i64, model_id: &str) -> Result<()> {
        sqlx::query("UPDATE match_run SET model_id = ? WHERE id = ?")
            .bind(model_id)
            .bind(run_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_run_status(&self, run_id: i64, status: RunStatus) -> Result<()> {
        sqlx::query("UPDATE match_run SET status = ? WHERE id = ?")
            .bind(status_str(status))
            .bind(run_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn insert_scores(&self, rows: &[ScoreRow]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for r in rows {
            sqlx::query(
                "INSERT INTO match_score (run_id, target_type, target_id, question, probabilities_json, score)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(r.run_id)
            .bind(target_str(r.target_type))
            .bind(r.target_id)
            .bind(&r.question)
            .bind(serde_json::to_string(&r.probabilities)?)
            .bind(r.score)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn scores_for_run(&self, run_id: i64) -> Result<Vec<ScoreRow>> {
        let rows = sqlx::query("SELECT * FROM match_score WHERE run_id = ? ORDER BY rowid")
            .bind(run_id)
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(|r| {
                let probabilities: BTreeMap<String, f64> =
                    serde_json::from_str(r.get::<&str, _>("probabilities_json"))?;
                Ok(ScoreRow {
                    run_id: r.get("run_id"),
                    target_type: parse_target(r.get("target_type"))?,
                    target_id: r.get("target_id"),
                    question: r.get("question"),
                    probabilities,
                    score: r.get("score"),
                })
            })
            .collect()
    }

    // ---- settings ----

    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get("value")))
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_setting(&self, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---- secrets ----

    pub async fn get_secret(&self, provider: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT api_key FROM secrets WHERE provider = ?")
            .bind(provider)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get("api_key")))
    }

    pub async fn set_secret(&self, provider: &str, api_key: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO secrets (provider, api_key, updated_at)
             VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
             ON CONFLICT(provider) DO UPDATE SET api_key = excluded.api_key, updated_at = excluded.updated_at",
        )
        .bind(provider)
        .bind(api_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_secret(&self, provider: &str) -> Result<()> {
        sqlx::query("DELETE FROM secrets WHERE provider = ?")
            .bind(provider)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // ---- cvs ----

    pub async fn save_cv(&self, cv: &CvInput) -> Result<i64> {
        Ok(sqlx::query(
            "INSERT INTO cv (jd_id, run_id, template, page_budget, selection_json, pdf_path)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(cv.jd_id)
        .bind(cv.run_id)
        .bind(&cv.template)
        .bind(cv.page_budget)
        .bind(&cv.selection_json)
        .bind(&cv.pdf_path)
        .execute(&self.pool)
        .await?
        .last_insert_rowid())
    }

    pub async fn list_cvs(&self, jd_id: i64) -> Result<Vec<Cv>> {
        let rows = sqlx::query("SELECT * FROM cv WHERE jd_id = ? ORDER BY id DESC")
            .bind(jd_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(cv_from_row).collect())
    }
}
