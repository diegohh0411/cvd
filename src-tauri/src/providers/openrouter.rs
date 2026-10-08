use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use super::{Provider, ProviderError, RemoteModel};
use crate::engine::{DecisionEngine, HttpSystemOneEngine};

pub const OPENROUTER_BASE: &str = "https://openrouter.ai/api";
pub const OPENROUTER: &str = "openrouter";

pub struct OpenRouterProvider {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl OpenRouterProvider {
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
            return Err(ProviderError::Unauthorized);
        }
        if !status.is_success() {
            return Err(ProviderError::BadResponse(format!("HTTP {status}")));
        }
        resp.json().await.map_err(|e| ProviderError::BadResponse(e.to_string()))
    }

    /// The models list answers 200 even for invalid keys, so validate against `/v1/key`.
    pub async fn validate_key(&self) -> Result<(), ProviderError> {
        self.get("/v1/key").await.map(|_| ())
    }
}

fn to_model(m: &Value) -> Option<RemoteModel> {
    let id = m.get("id")?.as_str()?.to_string();
    Some(RemoteModel {
        provider: OPENROUTER.into(),
        display_name: m.get("name").and_then(Value::as_str).unwrap_or(&id).to_string(),
        context_length: m.get("context_length").and_then(Value::as_u64).map(|n| n as u32),
        price_per_mtok_in: m
            .pointer("/pricing/prompt")
            .and_then(Value::as_str)
            .and_then(|p| p.parse::<f64>().ok())
            .map(|p| p * 1e6),
        id,
    })
}

#[async_trait]
impl Provider for OpenRouterProvider {
    fn id(&self) -> &'static str {
        OPENROUTER
    }

    async fn list_models(&self) -> Result<Vec<RemoteModel>, ProviderError> {
        let v = self.get("/v1/models?output_modalities=decisions").await?;
        let data = v
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| ProviderError::BadResponse("missing `data` array".into()))?;
        Ok(data.iter().filter_map(to_model).collect())
    }

    fn engine(&self, model_id: &str) -> Result<Arc<dyn DecisionEngine>, ProviderError> {
        Ok(Arc::new(HttpSystemOneEngine::new(self.base_url.clone(), self.api_key.clone(), model_id.into())))
    }
}
