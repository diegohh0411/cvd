use super::super::{Result, UserId};
use sqlx::{PgConnection, Row};
use std::collections::BTreeMap;
pub(in crate::store) async fn apply_memory(
    conn: &mut PgConnection,
    user: UserId,
) -> Result<BTreeMap<String, String>> {
    let rows = sqlx::query("SELECT label_norm, fact_key FROM (SELECT * FROM apply_memory WHERE user_id = $1) apply_memory").bind(user.0)
            .fetch_all(&mut *conn)
            .await?;
    Ok(rows
        .iter()
        .map(|r| (r.get("label_norm"), r.get("fact_key")))
        .collect())
}

/// Records (or re-points) a label; `hits` counts how often it was confirmed.
pub(in crate::store) async fn remember_label(
    conn: &mut PgConnection,
    user: UserId,
    label_norm: &str,
    fact_key: &str,
) -> Result<()> {
    sqlx::query(
            "INSERT INTO apply_memory (user_id, label_norm, fact_key) VALUES ($1, $2, $3)
             ON CONFLICT(user_id, label_norm) DO UPDATE SET
               hits = CASE WHEN apply_memory.fact_key = excluded.fact_key THEN apply_memory.hits + 1 ELSE 1 END,
               fact_key = excluded.fact_key,
               updated_at = CURRENT_TIMESTAMP",
        ).bind(user.0)
        .bind(label_norm)
        .bind(fact_key)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub(in crate::store) async fn forget_label(
    conn: &mut PgConnection,
    user: UserId,
    label_norm: &str,
) -> Result<()> {
    sqlx::query("DELETE FROM apply_memory WHERE user_id = $1 AND label_norm = $2")
        .bind(user.0)
        .bind(label_norm)
        .execute(&mut *conn)
        .await?;
    Ok(())
}
