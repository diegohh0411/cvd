//! Postgres pool ownership and transaction-local tenant context.
mod apply_memory;
mod companies;
mod email;
mod notify;
mod queries;
use super::{Result, StoreError, UserId};
pub(super) use apply_memory::*;
pub(super) use companies::*;
pub(super) use email::*;
pub use notify::TABLES as CHANGE_TABLES;
pub use notify::{ChangeFeed, ChangeReceiver};
pub(super) use queries::*;
use sqlx::postgres::{PgPoolOptions, PgRow};
use sqlx::{PgPool, Postgres, Row, Transaction};

#[derive(Clone)]
pub(super) struct Store {
    pool: PgPool,
}
impl Store {
    pub(in crate::store) async fn open(url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;
        let unsafe_role: bool = sqlx::query_scalar(
            "SELECT rolsuper OR rolbypassrls FROM pg_roles WHERE rolname = current_user",
        )
        .fetch_one(&pool)
        .await?;
        if unsafe_role {
            pool.close().await;
            return Err(StoreError::Invalid(
                "Postgres requires a role without superuser or BYPASSRLS privileges".into(),
            ));
        }
        Ok(Self { pool })
    }
    pub(in crate::store) async fn migrate(url: &str) -> Result<()> {
        let pool = PgPoolOptions::new().max_connections(1).connect(url).await?;
        let result = sqlx::migrate!("./migrations/postgres").run(&pool).await;
        pool.close().await;
        result?;
        Ok(())
    }
    pub(in crate::store) async fn create_user_defaults(&self, user: UserId) -> Result<()> {
        let mut tx = self.begin(user).await?;
        sqlx::query(
            "INSERT INTO job_status(user_id, name, color, position) VALUES
            ($1, 'Bookmarked', 'blue', 0), ($1, 'Applied', 'yellow', 1),
            ($1, 'Interviewing', 'purple', 2), ($1, 'Offer', 'green', 3)",
        )
        .bind(user.0)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO job_view(user_id, name, layout, position) VALUES ($1, 'All jobs', 'table', 0)")
            .bind(user.0).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(in crate::store) async fn begin(&self, user: UserId) -> Result<Transaction<'_, Postgres>> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET LOCAL search_path = public, extensions")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET LOCAL TIME ZONE 'UTC'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT set_config('app.user_id', $1, true)")
            .bind(user.0.to_string())
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }
}
fn timestamp(r: &PgRow, column: &str) -> String {
    r.get::<chrono::DateTime<chrono::Utc>, _>(column)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}
fn optional_timestamp(r: &PgRow, column: &str) -> Option<String> {
    r.get::<Option<chrono::DateTime<chrono::Utc>>, _>(column)
        .map(|v| v.format("%Y-%m-%d %H:%M:%S").to_string())
}
