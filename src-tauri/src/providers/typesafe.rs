use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use super::{ChatEndpoint, Provider, ProviderError, RemoteModel};
use crate::engine::{DecisionEngine, HttpSystemOneEngine};

pub const TYPESAFE_BASE: &str = "https://api.typesafe.ai";
pub const TYPESAFE: &str = "typesafe";
const TYPESAFE_NAME: &str = "TypeSafe";

/// Jev is billed per input token only (docs.typesafe.ai/models, Oct 2026).
const JEV_PRICE_PER_MTOK_IN: f64 = 0.042;
/// Default per-request context; the long-context header is not used.
const JEV_CONTEXT_LENGTH: u32 = 64_000;

pub struct TypeSafeProvider {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl TypeSafeProvider {
    pub fn new(base_url: String, api_key: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest client");
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            http,
        }
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
            return Err(ProviderError::KeyRejected(TYPESAFE_NAME));
        }
        if !status.is_success() {
            return Err(ProviderError::BadResponse(format!("HTTP {status}")));
        }
        resp.json()
            .await
            .map_err(|e| ProviderError::BadResponse(e.to_string()))
    }

    /// `GET /v1/models` answers 401 for a bad key.
    pub async fn validate_key(&self) -> Result<(), ProviderError> {
        self.get("/v1/models").await.map(|_| ())
    }
}

fn display_name(name: &str) -> String {
    match name {
        "jev-latest" => "Jev (stable)".into(),
        "jev-preview" => "Jev (preview)".into(),
        other => other.into(),
    }
}

#[async_trait]
impl Provider for TypeSafeProvider {
    fn id(&self) -> &'static str {
        TYPESAFE
    }

    /// Every TypeSafe model is a System One decision model.
    async fn list_models(&self) -> Result<Vec<RemoteModel>, ProviderError> {
        let v = self.get("/v1/models").await?;
        let models = v
            .get("models")
            .and_then(Value::as_array)
            .ok_or_else(|| ProviderError::BadResponse("missing `models` array".into()))?;
        Ok(models
            .iter()
            .filter_map(|m| m.get("name").and_then(Value::as_str))
            .map(|name| RemoteModel {
                provider: TYPESAFE.into(),
                id: name.into(),
                display_name: display_name(name),
                context_length: Some(JEV_CONTEXT_LENGTH),
                price_per_mtok_in: Some(JEV_PRICE_PER_MTOK_IN),
                vision: None,
            })
            .collect())
    }

    /// Jev cannot generate text.
    async fn list_chat_models(&self) -> Result<Vec<RemoteModel>, ProviderError> {
        Ok(vec![])
    }

    fn engine(&self, model_id: &str) -> Result<Arc<dyn DecisionEngine>, ProviderError> {
        Ok(Arc::new(HttpSystemOneEngine::new(
            TYPESAFE_NAME,
            self.base_url.clone(),
            self.api_key.clone(),
            model_id.into(),
        )))
    }

    fn chat_endpoint(&self, _model_id: &str) -> Result<ChatEndpoint, ProviderError> {
        Err(ProviderError::Missing(
            "Jev only makes decisions; pick an OpenRouter model for company research.".into(),
        ))
    }
}
