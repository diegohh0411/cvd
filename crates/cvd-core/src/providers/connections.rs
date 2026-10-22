//! Several keys per provider. A connection id stands in for the provider segment of a model ref
//! and is the `secrets` row key, so the first key of a provider keeps its bare id (`openrouter`)
//! and nothing already stored needs migrating; further keys are `<provider>~<n>`.
use serde::{Deserialize, Serialize};

use crate::store::{StoreError, TenantStore as Store};

pub const CONNECTIONS_KEY: &str = "connections";

/// A saved key's name. Rows exist for extra keys and for renamed default keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub provider: String,
    pub label: String,
}

/// The provider a connection id belongs to: `openrouter~2` -> `openrouter`.
pub fn kind_of(id: &str) -> &str {
    id.split_once('~').map_or(id, |(kind, _)| kind)
}

pub async fn load_rows(store: &Store) -> Result<Vec<Connection>, StoreError> {
    Ok(store
        .get_setting(CONNECTIONS_KEY)
        .await?
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default())
}

pub async fn save_rows(store: &Store, rows: &[Connection]) -> Result<(), StoreError> {
    let raw = serde_json::to_string(rows).expect("connections serialize");
    store.set_setting(CONNECTIONS_KEY, &raw).await
}

/// The smallest unused `<provider>~<n>` with n >= 2.
pub fn next_id(provider: &str, rows: &[Connection]) -> String {
    (2..)
        .map(|n| format!("{provider}~{n}"))
        .find(|id| !rows.iter().any(|r| &r.id == id))
        .expect("an unused id")
}
