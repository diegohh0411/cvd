//! Company profiles: the cache behind the research agent.
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

use super::types::{Company, CompanyProfile, CompanyStatus};
use super::{Result, Store, StoreError};

fn parse_company_status(s: &str) -> Result<CompanyStatus> {
    Ok(match s {
        "pending" => CompanyStatus::Pending,
        "ok" => CompanyStatus::Ok,
        "failed" => CompanyStatus::Failed,
        other => return Err(StoreError::Invalid(format!("company status {other}"))),
    })
}

fn company_from_row(r: &SqliteRow) -> Result<Company> {
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
            tech_stack: serde_json::from_str(r.get("tech_stack_json"))?,
            sources: serde_json::from_str(r.get("sources_json"))?,
        },
        status: parse_company_status(r.get("status"))?,
        researched_at: r.get("researched_at"),
        model_id: r.get("model_id"),
        fail_count: r.get("fail_count"),
        last_error: r.get("last_error"),
        retry_after: r.get("retry_after"),
        edited_at: r.get("edited_at"),
    })
}

/// Writes every profile column; callers add their own lifecycle columns.
const PROFILE_SET: &str = "summary = ?, industry = ?, size = ?, stage = ?, headquarters = ?,
     remote_policy = ?, products = ?, tech_stack_json = ?, sources_json = ?";

fn bind_profile<'q>(
    q: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    p: &'q CompanyProfile,
) -> Result<sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>> {
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

impl Store {
    /// Looks a company up by domain, then normalized name, then alias.
    pub async fn find_company(&self, domain: Option<&str>, normalized: &str) -> Result<Option<Company>> {
        if let Some(d) = domain {
            if let Some(r) = sqlx::query("SELECT * FROM company WHERE domain = ?")
                .bind(d)
                .fetch_optional(&self.pool)
                .await?
            {
                return company_from_row(&r).map(Some);
            }
        }
        if normalized.is_empty() {
            return Ok(None);
        }
        let r = sqlx::query(
            "SELECT * FROM company WHERE normalized_name = ?1
             UNION ALL
             SELECT c.* FROM company c JOIN company_alias a ON a.company_id = c.id WHERE a.normalized_name = ?1
             LIMIT 1",
        )
        .bind(normalized)
        .fetch_optional(&self.pool)
        .await?;
        r.as_ref().map(company_from_row).transpose()
    }

    pub async fn create_company(&self, name: &str, normalized: &str, domain: Option<&str>) -> Result<Company> {
        let id = sqlx::query("INSERT INTO company (name, normalized_name, domain) VALUES (?, ?, ?)")
            .bind(name)
            .bind(normalized)
            .bind(domain)
            .execute(&self.pool)
            .await?
            .last_insert_rowid();
        self.get_company(id).await
    }

    /// Errors with `RowNotFound` if it does not exist.
    pub async fn get_company(&self, id: i64) -> Result<Company> {
        let r = sqlx::query("SELECT * FROM company WHERE id = ?").bind(id).fetch_one(&self.pool).await?;
        company_from_row(&r)
    }

    /// Stores a research result, overwriting any manual edits.
    pub async fn save_research(
        &self,
        id: i64,
        canonical_name: &str,
        domain: Option<&str>,
        profile: &CompanyProfile,
        model_id: &str,
    ) -> Result<Company> {
        let sql = format!(
            "UPDATE company SET {PROFILE_SET}, name = ?, domain = COALESCE(?, domain), model_id = ?,
                 status = 'ok', researched_at = CURRENT_TIMESTAMP, fail_count = 0, last_error = '',
                 retry_after = NULL, edited_at = NULL
             WHERE id = ?"
        );
        bind_profile(sqlx::query(&sql), profile)?
            .bind(canonical_name)
            .bind(domain)
            .bind(model_id)
            .bind(id)
            .execute(&self.pool)
            .await?;
        self.get_company(id).await
    }

    pub async fn mark_research_failed(&self, id: i64, error: &str, retry_after: &str) -> Result<()> {
        sqlx::query(
            "UPDATE company SET status = 'failed', fail_count = fail_count + 1, last_error = ?, retry_after = ?
             WHERE id = ?",
        )
        .bind(error)
        .bind(retry_after)
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// A manual edit; keeps `researched_at`, so the TTL still applies.
    pub async fn edit_company(&self, id: i64, profile: &CompanyProfile) -> Result<Company> {
        let sql = format!("UPDATE company SET {PROFILE_SET}, edited_at = CURRENT_TIMESTAMP WHERE id = ?");
        bind_profile(sqlx::query(&sql), profile)?.bind(id).execute(&self.pool).await?;
        self.get_company(id).await
    }

    pub async fn add_alias(&self, normalized: &str, company_id: i64) -> Result<()> {
        sqlx::query("INSERT OR IGNORE INTO company_alias (normalized_name, company_id) VALUES (?, ?)")
            .bind(normalized)
            .bind(company_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Folds `from` into `into`: aliases, jobs and `from`'s own name move over.
    pub async fn merge_company(&self, from: i64, into: i64) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let name: String = sqlx::query_scalar("SELECT normalized_name FROM company WHERE id = ?")
            .bind(from)
            .fetch_one(&mut *tx)
            .await?;
        for sql in [
            "UPDATE company_alias SET company_id = ?1 WHERE company_id = ?2",
            "UPDATE job_description SET company_id = ?1 WHERE company_id = ?2",
        ] {
            sqlx::query(sql).bind(into).bind(from).execute(&mut *tx).await?;
        }
        sqlx::query("DELETE FROM company WHERE id = ?").bind(from).execute(&mut *tx).await?;
        sqlx::query("INSERT OR IGNORE INTO company_alias (normalized_name, company_id) VALUES (?, ?)")
            .bind(&name)
            .bind(into)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn set_jd_company(&self, jd_id: i64, company_id: Option<i64>) -> Result<()> {
        sqlx::query("UPDATE job_description SET company_id = ? WHERE id = ?")
            .bind(company_id)
            .bind(jd_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_run_company(&self, run_id: i64, researched_at: Option<&str>) -> Result<()> {
        sqlx::query("UPDATE match_run SET company_researched_at = ? WHERE id = ?")
            .bind(researched_at)
            .bind(run_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
