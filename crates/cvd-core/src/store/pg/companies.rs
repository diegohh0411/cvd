use super::super::{types::*, Result, StoreError, UserId};
use super::optional_timestamp;
use sqlx::postgres::PgRow;
use sqlx::{PgConnection, Row};
fn parse_company_status(s: &str) -> Result<CompanyStatus> {
    Ok(match s {
        "pending" => CompanyStatus::Pending,
        "ok" => CompanyStatus::Ok,
        "failed" => CompanyStatus::Failed,
        other => return Err(StoreError::Invalid(format!("company status {other}"))),
    })
}

fn company_from_row(r: &PgRow) -> Result<Company> {
    Ok(Company {
        id: r.get("id"),
        name: r.get("name"),
        normalized_name: r.get("normalized_name"),
        domain: r.get("domain"),
        profile: CompanyProfile {
            summary: r.get("summary"),
            industry: r.get("industry"),
            size: r.get("size"),
            stage: r.get("stage"),
            headquarters: r.get("headquarters"),
            remote_policy: r.get("remote_policy"),
            products: r.get("products"),
            tech_stack: r.get::<sqlx::types::Json<_>, _>("tech_stack_json").0,
            sources: r.get::<sqlx::types::Json<_>, _>("sources_json").0,
        },
        status: parse_company_status(r.get("status"))?,
        researched_at: optional_timestamp(r, "researched_at"),
        model_id: r.get("model_id"),
        fail_count: r.get("fail_count"),
        last_error: r.get("last_error"),
        retry_after: optional_timestamp(r, "retry_after"),
        edited_at: optional_timestamp(r, "edited_at"),
    })
}

fn bind_profile<'q>(
    q: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    p: &'q CompanyProfile,
) -> Result<sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>> {
    Ok(q.bind(&p.summary)
        .bind(&p.industry)
        .bind(&p.size)
        .bind(&p.stage)
        .bind(&p.headquarters)
        .bind(&p.remote_policy)
        .bind(&p.products)
        .bind(serde_json::to_string(&p.tech_stack)?)
        .bind(serde_json::to_string(&p.sources)?))
}

/// Looks a company up by domain, then normalized name, then alias.
pub(in crate::store) async fn find_company(
    conn: &mut PgConnection,
    user: UserId,
    domain: Option<&str>,
    normalized: &str,
) -> Result<Option<Company>> {
    if let Some(d) = domain {
        if let Some(r) = sqlx::query(
            "SELECT * FROM (SELECT * FROM company WHERE user_id = $1) company WHERE domain = $2",
        )
        .bind(user.0)
        .bind(d)
        .fetch_optional(&mut *conn)
        .await?
        {
            return company_from_row(&r).map(Some);
        }
    }
    if normalized.is_empty() {
        return Ok(None);
    }
    let r = sqlx::query(
            "SELECT * FROM (SELECT * FROM company WHERE user_id = $1) company WHERE normalized_name = $2
             UNION ALL
             SELECT c.* FROM (SELECT * FROM company WHERE user_id = $1) c JOIN (SELECT * FROM company_alias WHERE user_id = $1) a ON a.company_id = c.id WHERE a.normalized_name = $2
             LIMIT 1",
        ).bind(user.0)
        .bind(normalized)
        .fetch_optional(&mut *conn)
        .await?;
    r.as_ref().map(company_from_row).transpose()
}

pub(in crate::store) async fn create_company(
    conn: &mut PgConnection,
    user: UserId,
    name: &str,
    normalized: &str,
    domain: Option<&str>,
) -> Result<Company> {
    let id =
            sqlx::query_scalar("INSERT INTO company (user_id, name, normalized_name, domain) VALUES ($1, $2, $3, $4) RETURNING id").bind(user.0)
                .bind(name)
                .bind(normalized)
                .bind(domain)
                .fetch_one(&mut *conn).await?;
    get_company(conn, user, id).await
}

/// Errors with `RowNotFound` if it does not exist.
pub(in crate::store) async fn get_company(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
) -> Result<Company> {
    let r = sqlx::query(
        "SELECT * FROM (SELECT * FROM company WHERE user_id = $1) company WHERE id = $2",
    )
    .bind(user.0)
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    company_from_row(&r)
}

/// Stores a research result, overwriting any manual edits.
pub(in crate::store) async fn save_research(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
    canonical_name: &str,
    domain: Option<&str>,
    profile: &CompanyProfile,
    model_id: &str,
) -> Result<Company> {
    let sql =
            "UPDATE company SET summary = $2, industry = $3, size = $4, stage = $5, headquarters = $6, remote_policy = $7, products = $8,  tech_stack_json = $9::text::jsonb,  sources_json = $10::text::jsonb, name = $11, domain = COALESCE($12, domain), model_id = $13,
                 status = 'ok', researched_at = CURRENT_TIMESTAMP, fail_count = 0, last_error = '',
                 retry_after = NULL, edited_at = NULL
             WHERE user_id = $1 AND id = $14"
        ;
    bind_profile(sqlx::query(sql).bind(user.0), profile)?
        .bind(canonical_name)
        .bind(domain)
        .bind(model_id)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    get_company(conn, user, id).await
}

pub(in crate::store) async fn mark_research_failed(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
    error: &str,
    retry_after: &str,
) -> Result<()> {
    sqlx::query(
            "UPDATE company SET status = 'failed', fail_count = fail_count + 1, last_error = $2,  retry_after = $3::text::timestamptz
             WHERE user_id = $1 AND id = $4",
        ).bind(user.0)
        .bind(error)
        .bind(retry_after)
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// A manual edit; keeps `researched_at`, so the TTL still applies.
pub(in crate::store) async fn edit_company(
    conn: &mut PgConnection,
    user: UserId,
    id: i64,
    profile: &CompanyProfile,
) -> Result<Company> {
    let sql =
            "UPDATE company SET summary = $2, industry = $3, size = $4, stage = $5, headquarters = $6, remote_policy = $7, products = $8,  tech_stack_json = $9::text::jsonb,  sources_json = $10::text::jsonb, edited_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND id = $11";
    bind_profile(sqlx::query(sql).bind(user.0), profile)?
        .bind(id)
        .execute(&mut *conn)
        .await?;
    get_company(conn, user, id).await
}

pub(in crate::store) async fn add_alias(
    conn: &mut PgConnection,
    user: UserId,
    normalized: &str,
    company_id: i64,
) -> Result<()> {
    sqlx::query(
            "INSERT INTO company_alias (user_id, normalized_name, company_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        ).bind(user.0)
        .bind(normalized)
        .bind(company_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// Folds `from` into `into`: aliases, jobs and `from`'s own name move over.
pub(in crate::store) async fn merge_company(
    conn: &mut PgConnection,
    user: UserId,
    from: i64,
    into: i64,
) -> Result<()> {
    let name: String = sqlx::query_scalar("SELECT normalized_name FROM (SELECT * FROM company WHERE user_id = $1) company WHERE id = $2").bind(user.0)
            .bind(from)
            .fetch_one(&mut *conn)
            .await?;
    for sql in [
        "UPDATE company_alias SET company_id = $2 WHERE user_id = $1 AND company_id = $3",
        "UPDATE job_description SET company_id = $2 WHERE user_id = $1 AND company_id = $3",
    ] {
        sqlx::query(sql)
            .bind(user.0)
            .bind(into)
            .bind(from)
            .execute(&mut *conn)
            .await?;
    }
    sqlx::query("DELETE FROM company WHERE user_id = $1 AND id = $2")
        .bind(user.0)
        .bind(from)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
            "INSERT INTO company_alias (user_id, normalized_name, company_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        ).bind(user.0)
        .bind(&name)
        .bind(into)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub(in crate::store) async fn set_jd_company(
    conn: &mut PgConnection,
    user: UserId,
    jd_id: i64,
    company_id: Option<i64>,
) -> Result<()> {
    sqlx::query("UPDATE job_description SET company_id = $2 WHERE user_id = $1 AND id = $3")
        .bind(user.0)
        .bind(company_id)
        .bind(jd_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub(in crate::store) async fn set_run_company(
    conn: &mut PgConnection,
    user: UserId,
    run_id: i64,
    researched_at: Option<&str>,
) -> Result<()> {
    sqlx::query("UPDATE match_run SET  company_researched_at = $2::text::timestamptz WHERE user_id = $1 AND id = $3").bind(user.0)
            .bind(researched_at)
            .bind(run_id)
            .execute(&mut *conn)
            .await?;
    Ok(())
}
