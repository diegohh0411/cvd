use super::super::{types::*, Result, StoreError, UserId};
use super::{optional_timestamp, timestamp};
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, Row};
use std::collections::BTreeMap;

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

fn job_status_from_row(r: &PgRow) -> Result<JobStatus> {
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

fn job_view_from_row(r: &PgRow) -> Result<JobView> {
    let layout: String = r.get("layout");
    Ok(JobView {
        id: r.get("id"),
        name: r.get("name"),
        layout: serde_json::from_value(serde_json::Value::String(layout.clone()))
            .map_err(|_| StoreError::Invalid(format!("view layout {layout}")))?,
        filters: r.get::<sqlx::types::Json<_>, _>("filters").0,
        sorts: r.get::<sqlx::types::Json<_>, _>("sorts").0,
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

fn skill_from_row(r: &PgRow) -> Skill {
    Skill {
        id: r.get("id"),
        name: r.get("name"),
        category: r.get("category"),
    }
}

fn jd_from_row(r: &PgRow) -> JobDescription {
    JobDescription {
        id: r.get("id"),
        title: r.get("title"),
        company: r.get("company"),
        raw_text: r.get("raw_text"),
        created_at: timestamp(r, "created_at"),
        url: r.get("url"),
        status_id: r.get("status_id"),
        company_id: r.get("company_id"),
    }
}

fn run_from_row(r: &PgRow) -> Result<MatchRun> {
    Ok(MatchRun {
        id: r.get("id"),
        jd_id: r.get("jd_id"),
        model_id: r.get("model_id"),
        status: parse_status(r.get("status"))?,
        created_at: timestamp(r, "created_at"),
        company_researched_at: optional_timestamp(r, "company_researched_at"),
    })
}

fn cv_from_row(r: &PgRow) -> Cv {
    Cv {
        id: r.get("id"),
        jd_id: r.get("jd_id"),
        run_id: r.get("run_id"),
        template: r.get("template"),
        page_budget: r.get("page_budget"),
        selection_json: r
            .get::<sqlx::types::Json<serde_json::Value>, _>("selection_json")
            .0
            .to_string(),
        pdf_path: r.get("pdf_path"),
        created_at: timestamp(r, "created_at"),
    }
}

// ---- profile ----

pub(in crate::store) async fn get_profile(
    conn: &mut PgConnection,
    user: UserId,
) -> Result<Option<Profile>> {
    let row = sqlx::query(
        "SELECT * FROM (SELECT * FROM profile WHERE user_id = $1) profile WHERE id = 1",
    )
    .bind(user.0)
    .fetch_optional(&mut *conn)
    .await?;
    row.map(|r| {
        Ok(Profile {
            name: r.get("name"),
            headline: r.get("headline"),
            email: r.get("email"),
            phone: r.get("phone"),
            location: r.get("location"),
            links: r.get::<sqlx::types::Json<_>, _>("links_json").0,
            summary: r.get("summary"),
        })
    })
    .transpose()
}

pub(in crate::store) async fn upsert_profile(
    conn: &mut PgConnection,
    user: UserId,
    p: &Profile,
) -> Result<()> {
    sqlx::query(
            "INSERT INTO profile (user_id, id, name, headline, email, phone, location, links_json, summary)
             VALUES ($1, 1, $2, $3, $4, $5, $6, $7::text::jsonb, $8)
             ON CONFLICT(user_id, id) DO UPDATE SET name = excluded.name, headline = excluded.headline,
               email = excluded.email, phone = excluded.phone, location = excluded.location,
               links_json = excluded.links_json, summary = excluded.summary",
        ).bind(user.0)
        .bind(&p.name)
        .bind(&p.headline)
        .bind(&p.email)
        .bind(&p.phone)
        .bind(&p.location)
        .bind(serde_json::to_string(&p.links)?)
        .bind(&p.summary)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

// ---- entries ----

pub(in crate::store) async fn list_entries(
    conn: &mut PgConnection,
    user: UserId,
    include_archived: bool,
) -> Result<Vec<Entry>> {
    let rows = sqlx::query(
            "SELECT * FROM (SELECT * FROM entry WHERE user_id = $1) entry WHERE ($2 OR archived = false) ORDER BY start_date DESC, id",
        ).bind(user.0)
        .bind(include_archived)
        .fetch_all(&mut *conn)
        .await?;
    let mut entries = Vec::with_capacity(rows.len());
    for r in rows {
        let id: i64 = r.get("id");
        let bullets = sqlx::query(
                "SELECT id, entry_id, text, position FROM (SELECT * FROM bullet WHERE user_id = $1) bullet WHERE entry_id = $2 ORDER BY position, id",
            ).bind(user.0)
            .bind(id)
            .fetch_all(&mut *conn)
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
            "SELECT s.id, s.name, s.category FROM (SELECT * FROM skill WHERE user_id = $1) s
                 JOIN (SELECT * FROM entry_skill WHERE user_id = $1) es ON es.skill_id = s.id
                 WHERE es.entry_id = $2 ORDER BY s.name",
        )
        .bind(user.0)
        .bind(id)
        .fetch_all(&mut *conn)
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
pub(in crate::store) async fn upsert_entry(
    conn: &mut PgConnection,
    user: UserId,
    e: &EntryInput,
) -> Result<i64> {
    let id = match e.id {
            Some(id) => {
                let res = sqlx::query(
                    "UPDATE entry SET kind = $2, title = $3, org = $4, location = $5, start_date = $6,
                       end_date = $7, description = $8, sort_hint = $9, archived = $10 WHERE user_id = $1 AND id = $11",
                ).bind(user.0)
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
                .execute(&mut *conn)
                .await?;
                if res.rows_affected() == 0 {
                    return Err(StoreError::Invalid(format!("no entry with id {id}")));
                }
                sqlx::query("DELETE FROM bullet WHERE user_id = $1 AND entry_id = $2").bind(user.0)
                    .bind(id)
                    .execute(&mut *conn)
                    .await?;
                sqlx::query("DELETE FROM entry_skill WHERE user_id = $1 AND entry_id = $2").bind(user.0)
                    .bind(id)
                    .execute(&mut *conn)
                    .await?;
                id
            }
            None => sqlx::query_scalar(
                "INSERT INTO entry (user_id, kind, title, org, location, start_date, end_date, description, sort_hint, archived)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
            ).bind(user.0)
            .bind(kind_str(e.kind))
            .bind(&e.title)
            .bind(&e.org)
            .bind(&e.location)
            .bind(&e.start_date)
            .bind(&e.end_date)
            .bind(&e.description)
            .bind(e.sort_hint)
            .bind(e.archived)
            .fetch_one(&mut *conn).await?,
        };
    for (pos, text) in e.bullets.iter().enumerate() {
        sqlx::query(
            "INSERT INTO bullet (user_id, entry_id, text, position) VALUES ($1, $2, $3, $4)",
        )
        .bind(user.0)
        .bind(id)
        .bind(text)
        .bind(pos as i64)
        .execute(&mut *conn)
        .await?;
    }
    for name in &e.skills {
        sqlx::query("INSERT INTO skill (user_id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(user.0)
            .bind(name)
            .execute(&mut *conn)
            .await?;
        // skill.name is COLLATE NOCASE, so this matches case-insensitively.
        sqlx::query(
                "INSERT INTO entry_skill (user_id, entry_id, skill_id)
                 SELECT $1, $2, id FROM (SELECT * FROM skill WHERE user_id = $1) skill WHERE name = $3::citext ON CONFLICT DO NOTHING",
            ).bind(user.0)
            .bind(id)
            .bind(name)
            .execute(&mut *conn)
            .await?;
    }
    Ok(id)
}

/// Finds an entry by its natural key; title and org match case-insensitively.
pub(in crate::store) async fn find_entry_by_key(
    conn: &mut PgConnection,
    user: UserId,
    kind: EntryKind,
    title: &str,
    org: &str,
    start_date: &str,
) -> Result<Option<i64>> {
    let id = sqlx::query_scalar(
            "SELECT id FROM (SELECT * FROM entry WHERE user_id = $1) entry WHERE kind = $2 AND title = $3::citext
               AND org = $4::citext AND start_date = $5 LIMIT 1",
        ).bind(user.0)
        .bind(kind_str(kind))
        .bind(title)
        .bind(org)
        .bind(start_date)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(id)
}

pub(in crate::store) async fn delete_entry(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
) -> Result<()> {
    sqlx::query("DELETE FROM entry WHERE user_id = $1 AND id = $2")
        .bind(user.0)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub(in crate::store) async fn list_skills(
    conn: &mut PgConnection,
    user: UserId,
) -> Result<Vec<Skill>> {
    let rows = sqlx::query("SELECT id, name, category FROM (SELECT * FROM skill WHERE user_id = $1) skill ORDER BY name").bind(user.0)
            .fetch_all(&mut *conn)
            .await?;
    Ok(rows.iter().map(skill_from_row).collect())
}

/// Renames and recategorizes a skill. Renaming onto another skill's name
/// (case-insensitive) merges into it: entries move over and `id` is
/// deleted. Returns the surviving skill, or None when `id` does not exist.
pub(in crate::store) async fn update_skill(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
    name: &str,
    category: &str,
) -> Result<Option<Skill>> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::Invalid("skill name must not be empty".into()));
    }
    if sqlx::query("SELECT 1 FROM (SELECT * FROM skill WHERE user_id = $1) skill WHERE id = $2")
        .bind(user.0)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .is_none()
    {
        return Ok(None);
    }
    let other: Option<i64> =
            sqlx::query_scalar("SELECT id FROM (SELECT * FROM skill WHERE user_id = $1) skill WHERE name = $2::citext AND id != $3").bind(user.0)
                .bind(name)
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?;
    let target = match other {
        Some(target) => {
            sqlx::query(
                    "INSERT INTO entry_skill (user_id, entry_id, skill_id)
                     SELECT $1, entry_id, $2 FROM (SELECT * FROM entry_skill WHERE user_id = $1) entry_skill WHERE skill_id = $3 ON CONFLICT DO NOTHING",
                ).bind(user.0)
                .bind(target)
                .bind(id)
                .execute(&mut *conn)
                .await?;
            sqlx::query("DELETE FROM skill WHERE user_id = $1 AND id = $2")
                .bind(user.0)
                .bind(id)
                .execute(&mut *conn)
                .await?;
            target
        }
        None => id,
    };
    sqlx::query("UPDATE skill SET name = $2, category = $3 WHERE user_id = $1 AND id = $4")
        .bind(user.0)
        .bind(name)
        .bind(category.trim())
        .bind(target)
        .execute(&mut *conn)
        .await?;
    let row = sqlx::query("SELECT id, name, category FROM (SELECT * FROM skill WHERE user_id = $1) skill WHERE id = $2").bind(user.0)
            .bind(target)
            .fetch_one(&mut *conn)
            .await?;
    Ok(Some(skill_from_row(&row)))
}

/// Deletes a skill and unlinks it from every entry. Returns false when it does not exist.
pub(in crate::store) async fn delete_skill(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
) -> Result<bool> {
    let done = sqlx::query("DELETE FROM skill WHERE user_id = $1 AND id = $2")
        .bind(user.0)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(done.rows_affected() > 0)
}

// ---- job descriptions ----

/// Inserts a job. `status_id: None` picks the first status (e.g. Bookmarked).
pub(in crate::store) async fn create_jd(
    conn: &mut PgConnection,
    user: UserId,
    title: &str,
    company: &str,
    raw_text: &str,
    url: &str,
    status_id: Option<i64>,
) -> Result<i64> {
    let status_id = match status_id {
            Some(id) => Some(id),
            None => {
                sqlx::query_scalar("SELECT id FROM (SELECT * FROM job_status WHERE user_id = $1) job_status ORDER BY position, id LIMIT 1").bind(user.0)
                    .fetch_optional(&mut *conn)
                    .await?
            }
        };
    Ok(sqlx::query_scalar(
            "INSERT INTO job_description (user_id, title, company, raw_text, url, status_id) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        ).bind(user.0)
        .bind(title)
        .bind(company)
        .bind(raw_text)
        .bind(url.trim())
        .bind(status_id)
        .fetch_one(&mut *conn).await?)
}

/// Sets a job's tracking fields; `status_id: None` clears the status.
/// Returns false when the job does not exist.
pub(in crate::store) async fn update_jd_tracking(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
    status_id: Option<i64>,
    url: &str,
) -> Result<bool> {
    let done = sqlx::query(
        "UPDATE job_description SET status_id = $2, url = $3 WHERE user_id = $1 AND id = $4",
    )
    .bind(user.0)
    .bind(status_id)
    .bind(url.trim())
    .bind(id)
    .execute(&mut *conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Replaces a job's description text. Returns false when the job does not exist.
pub(in crate::store) async fn update_jd_text(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
    raw_text: &str,
) -> Result<bool> {
    let done =
        sqlx::query("UPDATE job_description SET raw_text = $2 WHERE user_id = $1 AND id = $3")
            .bind(user.0)
            .bind(raw_text)
            .bind(id)
            .execute(&mut *conn)
            .await?;
    Ok(done.rows_affected() > 0)
}

// ---- job statuses ----

pub(in crate::store) async fn list_job_statuses(
    conn: &mut PgConnection,
    user: UserId,
) -> Result<Vec<JobStatus>> {
    let rows = sqlx::query("SELECT * FROM (SELECT * FROM job_status WHERE user_id = $1) job_status ORDER BY position, id").bind(user.0)
            .fetch_all(&mut *conn)
            .await?;
    rows.iter().map(job_status_from_row).collect()
}

/// Case-insensitive lookup by name.
pub(in crate::store) async fn find_job_status(
    conn: &mut PgConnection,
    user: UserId,
    name: &str,
) -> Result<Option<JobStatus>> {
    let row = sqlx::query(
        "SELECT * FROM (SELECT * FROM job_status WHERE user_id = $1) job_status WHERE name = $2::citext",
    )
    .bind(user.0)
    .bind(name.trim())
    .fetch_optional(&mut *conn)
    .await?;
    row.as_ref().map(job_status_from_row).transpose()
}

/// Appends a status at the end of the list.
pub(in crate::store) async fn create_job_status(
    conn: &mut PgConnection,
    user: UserId,
    name: &str,
    color: StatusColor,
) -> Result<i64> {
    let name = status_name(name)?;
    Ok(sqlx::query_scalar(
            "INSERT INTO job_status (user_id, name, color, position)
             VALUES ($1, $2, $3, (SELECT COALESCE(MAX(position) + 1, 0) FROM (SELECT * FROM job_status WHERE user_id = $1) job_status)) RETURNING id",
        ).bind(user.0)
        .bind(name)
        .bind(color_str(color))
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| dup_status(e, name))?)
}

/// Renames/recolors a status. Returns false when it does not exist.
pub(in crate::store) async fn update_job_status(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
    name: &str,
    color: StatusColor,
) -> Result<bool> {
    let name = status_name(name)?;
    let done =
        sqlx::query("UPDATE job_status SET name = $2, color = $3 WHERE user_id = $1 AND id = $4")
            .bind(user.0)
            .bind(name)
            .bind(color_str(color))
            .bind(id)
            .execute(&mut *conn)
            .await
            .map_err(|e| dup_status(e, name))?;
    Ok(done.rows_affected() > 0)
}

/// Deletes a status; jobs using it lose their status.
pub(in crate::store) async fn delete_job_status(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
) -> Result<()> {
    sqlx::query("DELETE FROM job_status WHERE user_id = $1 AND id = $2")
        .bind(user.0)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Sets positions to the order of `ids`; ids not listed keep theirs.
pub(in crate::store) async fn reorder_job_statuses(
    conn: &mut PgConnection,
    user: UserId,
    ids: &[i64],
) -> Result<()> {
    for (pos, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE job_status SET position = $2 WHERE user_id = $1 AND id = $3")
            .bind(user.0)
            .bind(pos as i64)
            .bind(id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

// ---- job views ----

pub(in crate::store) async fn list_job_views(
    conn: &mut PgConnection,
    user: UserId,
) -> Result<Vec<JobView>> {
    let rows = sqlx::query(
        "SELECT * FROM (SELECT * FROM job_view WHERE user_id = $1) job_view ORDER BY position, id",
    )
    .bind(user.0)
    .fetch_all(&mut *conn)
    .await?;
    rows.iter().map(job_view_from_row).collect()
}

pub(in crate::store) async fn get_job_view(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
) -> Result<Option<JobView>> {
    let row = sqlx::query(
        "SELECT * FROM (SELECT * FROM job_view WHERE user_id = $1) job_view WHERE id = $2",
    )
    .bind(user.0)
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    row.as_ref().map(job_view_from_row).transpose()
}

/// Appends a view at the end of the tabs.
pub(in crate::store) async fn create_job_view(
    conn: &mut PgConnection,
    user: UserId,
    v: &JobViewInput,
) -> Result<i64> {
    let (name, layout, filters, sorts) = view_columns(v)?;
    Ok(sqlx::query_scalar(
            "INSERT INTO job_view (user_id, name, layout, filters, sorts, position)
             VALUES ($1, $2, $3, $4::text::jsonb, $5::text::jsonb, (SELECT COALESCE(MAX(position) + 1, 0) FROM (SELECT * FROM job_view WHERE user_id = $1) job_view)) RETURNING id",
        ).bind(user.0)
        .bind(name)
        .bind(layout)
        .bind(filters)
        .bind(sorts)
        .fetch_one(&mut *conn).await?)
}

/// Saves a view. Returns false when it does not exist.
pub(in crate::store) async fn update_job_view(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
    v: &JobViewInput,
) -> Result<bool> {
    let (name, layout, filters, sorts) = view_columns(v)?;
    let done = sqlx::query(
            "UPDATE job_view SET name = $2, layout = $3,  filters = $4::text::jsonb,  sorts = $5::text::jsonb WHERE user_id = $1 AND id = $6",
        ).bind(user.0)
        .bind(name)
        .bind(layout)
        .bind(filters)
        .bind(sorts)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Deletes a view, refusing to remove the last one so the page always has a view.
pub(in crate::store) async fn delete_job_view(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
) -> Result<()> {
    let others: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (SELECT * FROM job_view WHERE user_id = $1) job_view WHERE id != $2",
    )
    .bind(user.0)
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if others == 0 {
        return Err(StoreError::Invalid(
            "the last view cannot be deleted".into(),
        ));
    }
    sqlx::query("DELETE FROM job_view WHERE user_id = $1 AND id = $2")
        .bind(user.0)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub(in crate::store) async fn get_jd(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
) -> Result<Option<JobDescription>> {
    let row = sqlx::query("SELECT * FROM (SELECT * FROM job_description WHERE user_id = $1) job_description WHERE id = $2").bind(user.0)
            .bind(id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(row.as_ref().map(jd_from_row))
}

pub(in crate::store) async fn list_jds(
    conn: &mut PgConnection,
    user: UserId,
) -> Result<Vec<JobDescription>> {
    let rows = sqlx::query("SELECT * FROM (SELECT * FROM job_description WHERE user_id = $1) job_description ORDER BY created_at DESC, id DESC").bind(user.0)
            .fetch_all(&mut *conn)
            .await?;
    Ok(rows.iter().map(jd_from_row).collect())
}

// ---- match runs ----

/// Creates a run in `running` status.
pub(in crate::store) async fn create_run(
    conn: &mut PgConnection,
    user: UserId,
    jd_id: i64,
    model_id: &str,
) -> Result<i64> {
    Ok(
            sqlx::query_scalar("INSERT INTO match_run (user_id, jd_id, model_id, status) VALUES ($1, $2, $3, 'running') RETURNING id").bind(user.0)
                .bind(jd_id)
                .bind(model_id)
                .fetch_one(&mut *conn).await?,
        )
}

/// Fetches a run; errors with `RowNotFound` if it does not exist.
pub(in crate::store) async fn get_run(
    conn: &mut PgConnection,
    user: UserId,
    run_id: i64,
) -> Result<MatchRun> {
    let r = sqlx::query(
        "SELECT * FROM (SELECT * FROM match_run WHERE user_id = $1) match_run WHERE id = $2",
    )
    .bind(user.0)
    .bind(run_id)
    .fetch_one(&mut *conn)
    .await?;
    run_from_row(&r)
}

/// Runs for a job, newest first.
pub(in crate::store) async fn list_runs(
    conn: &mut PgConnection,
    user: UserId,
    jd_id: i64,
) -> Result<Vec<MatchRun>> {
    let rows = sqlx::query("SELECT * FROM (SELECT * FROM match_run WHERE user_id = $1) match_run WHERE jd_id = $2 ORDER BY id DESC").bind(user.0)
            .bind(jd_id)
            .fetch_all(&mut *conn)
            .await?;
    rows.iter().map(run_from_row).collect()
}

pub(in crate::store) async fn set_run_status(
    conn: &mut PgConnection,
    user: UserId,
    run_id: i64,
    status: RunStatus,
) -> Result<()> {
    sqlx::query("UPDATE match_run SET status = $2 WHERE user_id = $1 AND id = $3")
        .bind(user.0)
        .bind(status_str(status))
        .bind(run_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub(in crate::store) async fn insert_scores(
    conn: &mut PgConnection,
    user: UserId,
    rows: &[ScoreRow],
) -> Result<()> {
    for r in rows {
        sqlx::query(
                "INSERT INTO match_score (user_id, run_id, target_type, target_id, question, probabilities_json, score)
                 VALUES ($1, $2, $3, $4, $5, $6::text::jsonb, $7)",
            ).bind(user.0)
            .bind(r.run_id)
            .bind(target_str(r.target_type))
            .bind(r.target_id)
            .bind(&r.question)
            .bind(serde_json::to_string(&r.probabilities)?)
            .bind(r.score)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

pub(in crate::store) async fn scores_for_run(
    conn: &mut PgConnection,
    user: UserId,
    run_id: i64,
) -> Result<Vec<ScoreRow>> {
    let rows = sqlx::query("SELECT * FROM (SELECT * FROM match_score WHERE user_id = $1) match_score WHERE run_id = $2 ORDER BY id").bind(user.0)
            .bind(run_id)
            .fetch_all(&mut *conn)
            .await?;
    rows.iter()
        .map(|r| {
            let probabilities: BTreeMap<String, f64> =
                r.get::<sqlx::types::Json<_>, _>("probabilities_json").0;
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

pub(in crate::store) async fn get_setting(
    conn: &mut PgConnection,
    user: UserId,
    key: &str,
) -> Result<Option<String>> {
    let row = sqlx::query(
        "SELECT value FROM (SELECT * FROM settings WHERE user_id = $1) settings WHERE key = $2",
    )
    .bind(user.0)
    .bind(key)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row.map(|r| r.get("value")))
}

pub(in crate::store) async fn set_setting(
    conn: &mut PgConnection,
    user: UserId,
    key: &str,
    value: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO settings (user_id, key, value) VALUES ($1, $2, $3)
             ON CONFLICT(user_id, key) DO UPDATE SET value = excluded.value",
    )
    .bind(user.0)
    .bind(key)
    .bind(value)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(in crate::store) async fn delete_setting(
    conn: &mut PgConnection,
    user: UserId,
    key: &str,
) -> Result<()> {
    sqlx::query("DELETE FROM settings WHERE user_id = $1 AND key = $2")
        .bind(user.0)
        .bind(key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

// ---- secrets ----

pub(in crate::store) async fn get_secret(
    conn: &mut PgConnection,
    user: UserId,
    provider: &str,
) -> Result<Option<String>> {
    let row = sqlx::query("SELECT api_key FROM (SELECT * FROM secrets WHERE user_id = $1) secrets WHERE provider = $2").bind(user.0)
            .bind(provider)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(row.map(|r| r.get("api_key")))
}

pub(in crate::store) async fn set_secret(
    conn: &mut PgConnection,
    user: UserId,
    provider: &str,
    api_key: &str,
) -> Result<()> {
    sqlx::query(
            "INSERT INTO secrets (user_id, provider, api_key, updated_at)
             VALUES ($1, $2, $3, CURRENT_TIMESTAMP)
             ON CONFLICT(user_id, provider) DO UPDATE SET api_key = excluded.api_key, updated_at = excluded.updated_at",
        ).bind(user.0)
        .bind(provider)
        .bind(api_key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub(in crate::store) async fn delete_secret(
    conn: &mut PgConnection,
    user: UserId,
    provider: &str,
) -> Result<()> {
    sqlx::query("DELETE FROM secrets WHERE user_id = $1 AND provider = $2")
        .bind(user.0)
        .bind(provider)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

// ---- cvs ----

pub(in crate::store) async fn save_cv(
    conn: &mut PgConnection,
    user: UserId,
    cv: &CvInput,
) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "INSERT INTO cv (user_id, jd_id, run_id, template, page_budget, selection_json, pdf_path)
             VALUES ($1, $2, $3, $4, $5, $6::text::jsonb, $7) RETURNING id",
    )
    .bind(user.0)
    .bind(cv.jd_id)
    .bind(cv.run_id)
    .bind(&cv.template)
    .bind(cv.page_budget)
    .bind(&cv.selection_json)
    .bind(&cv.pdf_path)
    .fetch_one(&mut *conn)
    .await?)
}

pub(in crate::store) async fn list_cvs(
    conn: &mut PgConnection,
    user: UserId,
    jd_id: i64,
) -> Result<Vec<Cv>> {
    let rows = sqlx::query(
        "SELECT * FROM (SELECT * FROM cv WHERE user_id = $1) cv WHERE jd_id = $2 ORDER BY id DESC",
    )
    .bind(user.0)
    .bind(jd_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.iter().map(cv_from_row).collect())
}

pub(in crate::store) async fn change_versions(
    _conn: &mut PgConnection,
    _user: UserId,
) -> Result<BTreeMap<String, i64>> {
    Ok(BTreeMap::new())
}
