//! Auto-apply memory: normalized field label → fact key, learned from the user's edits.
use std::collections::BTreeMap;

use sqlx::Row;

use super::{Result, Store};

impl Store {
    pub async fn apply_memory(&self) -> Result<BTreeMap<String, String>> {
        let rows = sqlx::query("SELECT label_norm, fact_key FROM apply_memory").fetch_all(&self.pool).await?;
        Ok(rows.iter().map(|r| (r.get("label_norm"), r.get("fact_key"))).collect())
    }

    /// Records (or re-points) a label; `hits` counts how often it was confirmed.
    pub async fn remember_label(&self, label_norm: &str, fact_key: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO apply_memory (label_norm, fact_key) VALUES (?, ?)
             ON CONFLICT(label_norm) DO UPDATE SET
               hits = CASE WHEN fact_key = excluded.fact_key THEN hits + 1 ELSE 1 END,
               fact_key = excluded.fact_key,
               updated_at = CURRENT_TIMESTAMP",
        )
        .bind(label_norm)
        .bind(fact_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn forget_label(&self, label_norm: &str) -> Result<()> {
        sqlx::query("DELETE FROM apply_memory WHERE label_norm = ?").bind(label_norm).execute(&self.pool).await?;
        Ok(())
    }
}
