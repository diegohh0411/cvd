pub mod types;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
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
    }
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
    /// Opens (creating if needed) the database at `url` and runs migrations.
    /// In-memory URLs use a single connection so the database is shared.
    pub async fn open(url: &str) -> Result<Store> {
        let opts = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true)
            .foreign_keys(true);
        let mut pool_opts = SqlitePoolOptions::new();
        if url.contains(":memory:") {
            pool_opts = pool_opts.max_connections(1);
        }
        let pool = pool_opts.connect_with(opts).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
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

    // ---- job descriptions ----

    pub async fn create_jd(&self, title: &str, company: &str, raw_text: &str) -> Result<i64> {
        Ok(sqlx::query("INSERT INTO job_description (title, company, raw_text) VALUES (?, ?, ?)")
            .bind(title)
            .bind(company)
            .bind(raw_text)
            .execute(&self.pool)
            .await?
            .last_insert_rowid())
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
        let r = sqlx::query("SELECT id, jd_id, model_id, status FROM match_run WHERE id = ?")
            .bind(run_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(MatchRun {
            id: r.get("id"),
            jd_id: r.get("jd_id"),
            model_id: r.get("model_id"),
            status: parse_status(r.get("status"))?,
        })
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
