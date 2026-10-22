//! HTTP inference connections, including servers run locally by the user.
mod connections;
mod local;
mod openrouter;
mod registry;
mod subq;
mod typesafe;

use std::sync::Arc;

use async_trait::async_trait;

use crate::engine::DecisionEngine;

pub use connections::{kind_of, load_rows, next_id, save_rows, Connection};
pub use openrouter::{OpenRouterProvider, OPENROUTER, OPENROUTER_BASE};
pub use registry::{ProviderModels, ProviderRegistry};
pub use subq::{SubQProvider, SUBQ, SUBQ_BASE};
pub use typesafe::{TypeSafeProvider, TYPESAFE, TYPESAFE_BASE};

pub use local::{normalize_url, LocalProvider, DEFAULT_LOCAL_URL, LOCAL_SERVER, LOCAL_URL_KEY};

pub const LOCAL: &str = "local";

/// Model references use `<connection>:<id>` (including `localhost:<id>`).
/// Bare ids are decoded only for legacy history. Split on the first `:` since
/// model ids may themselves contain `:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    pub provider: String,
    pub id: String,
}

impl ModelRef {
    pub fn parse(s: &str) -> ModelRef {
        match s.split_once(':') {
            Some((provider, id)) => ModelRef {
                provider: provider.into(),
                id: id.into(),
            },
            None => ModelRef {
                provider: LOCAL.into(),
                id: s.into(),
            },
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn encode(&self) -> String {
        if self.provider == LOCAL {
            self.id.clone()
        } else {
            format!("{}:{}", self.provider, self.id)
        }
    }

    pub fn is_local(&self) -> bool {
        self.provider == LOCAL
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RemoteModel {
    pub provider: String,
    pub id: String,
    pub display_name: String,
    pub context_length: Option<u32>,
    /// USD per million input tokens.
    pub price_per_mtok_in: Option<f64>,
    /// Whether the model reads images, when the provider says. `None` means
    /// unknown: cvd probes it (autofill/agent/vision.rs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vision: Option<bool>,
}

/// `architecture.input_modalities` as OpenRouter and llama.cpp servers report it.
pub fn vision_from_architecture(m: &serde_json::Value) -> Option<bool> {
    let list = m.pointer("/architecture/input_modalities")?.as_array()?;
    Some(list.iter().any(|x| x.as_str() == Some("image")))
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProviderError {
    #[error("Check your OpenRouter key")]
    Unauthorized,
    #[error("Check your {0} key")]
    KeyRejected(&'static str),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Bad response from provider: {0}")]
    BadResponse(String),
    #[error("{0}")]
    Missing(String),
}

/// An OpenAI-compatible chat endpoint with tool calling (company research).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatEndpoint {
    /// Ends in `/v1`; requests go to `{base_url}/chat/completions`.
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    /// Decision (System One) models.
    async fn list_models(&self) -> Result<Vec<RemoteModel>, ProviderError>;
    /// Chat models that support tool calling.
    async fn list_chat_models(&self) -> Result<Vec<RemoteModel>, ProviderError>;
    fn engine(&self, model_id: &str) -> Result<Arc<dyn DecisionEngine>, ProviderError>;
    fn chat_endpoint(&self, model_id: &str) -> Result<ChatEndpoint, ProviderError>;
}

#[cfg(test)]
mod tests;
