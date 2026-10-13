use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use super::{ChatEndpoint, Provider, ProviderError, RemoteModel};
use crate::engine::{DecisionEngine, HttpSystemOneEngine};

pub const SUBQ_BASE: &str = "https://api.subq.ai";
pub const SUBQ: &str = "subq";
pub const SUBQ_SNAP: &str = "subq-snap-preview";

/// The models API has no pricing or context fields, so these come from
/// docs.subq.ai (Models and Decisions API, Oct 2026).
const SNAP_PRICE_PER_MTOK_IN: f64 = 0.05;
const SNAP_CONTEXT_LENGTH: u32 = 393_216;

/// `/v1/models` also lists chat-only models, which `/v1/systemone` rejects.
const DECISION_MODELS: &[&str] = &[SUBQ_SNAP];

pub struct SubQProvider {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl SubQProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).build().expect("reqwest client");
        Self { base_url: base_url.trim_end_matches('/').to_string(), api_key, http }
    }

    async fn get(&self, path: &str) -> Result<Value, ProviderError> {
        let resp = self
            .http
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::KeyRejected(SUBQ_NAME));
        }
        if !status.is_success() {
            return Err(ProviderError::BadResponse(format!("HTTP {status}")));
        }
        resp.json().await.map_err(|e| ProviderError::BadResponse(e.to_string()))
    }

    /// Checks the key against `/v1/models`, which rejects bad keys with 401.
    pub async fn validate_key(&self) -> Result<(), ProviderError> {
        self.get("/v1/models").await.map(|_| ())
    }
}

const SUBQ_NAME: &str = "SubQ";

#[async_trait]
impl Provider for SubQProvider {
    fn id(&self) -> &'static str {
        SUBQ
    }

    async fn list_models(&self) -> Result<Vec<RemoteModel>, ProviderError> {
        let v = self.get("/v1/models").await?;
        let data = v
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| ProviderError::BadResponse("missing `data` array".into()))?;
        Ok(data
            .iter()
            .filter_map(|m| m.get("id").and_then(Value::as_str))
            .filter(|id| DECISION_MODELS.contains(id))
            .map(|id| RemoteModel {
                provider: SUBQ.into(),
                id: id.into(),
                display_name: match id {
                    SUBQ_SNAP => "SubQ Snap".into(),
                    _ => id.into(),
                },
                context_length: Some(SNAP_CONTEXT_LENGTH),
                price_per_mtok_in: Some(SNAP_PRICE_PER_MTOK_IN),
            })
            .collect())
    }

    /// SubQ Snap is a decision model only; SubQ's chat model is not wired up yet.
    async fn list_chat_models(&self) -> Result<Vec<RemoteModel>, ProviderError> {
        Ok(vec![])
    }

    fn engine(&self, model_id: &str) -> Result<Arc<dyn DecisionEngine>, ProviderError> {
        Ok(Arc::new(HttpSystemOneEngine::new(SUBQ_NAME, self.base_url.clone(), self.api_key.clone(), model_id.into())))
    }

    fn chat_endpoint(&self, _model_id: &str) -> Result<ChatEndpoint, ProviderError> {
        Err(ProviderError::Missing("SubQ Snap only makes decisions; pick an OpenRouter model for company research.".into()))
    }
}
