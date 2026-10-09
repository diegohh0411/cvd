//! Inference providers: where a decision model runs. Local models keep going
//! through `AppState`'s sidecar path; remote providers implement `Provider`.
mod openrouter;
mod registry;

use std::sync::Arc;

use async_trait::async_trait;

use crate::engine::DecisionEngine;

pub use openrouter::{OpenRouterProvider, OPENROUTER, OPENROUTER_BASE};
pub use registry::{ProviderModels, ProviderRegistry};

pub const LOCAL: &str = "local";

/// A model reference as stored in settings and runs: bare id for local models,
/// `<provider>:<id>` for remote ones. Splits on the first `:` only, since
/// remote ids may contain `:` (e.g. `inception/mercury-decide:free`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRef {
    pub provider: String,
    pub id: String,
}

impl ModelRef {
    pub fn parse(s: &str) -> ModelRef {
        match s.split_once(':') {
            Some((provider, id)) => ModelRef { provider: provider.into(), id: id.into() },
            None => ModelRef { provider: LOCAL.into(), id: s.into() },
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

#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    async fn list_models(&self) -> Result<Vec<RemoteModel>, ProviderError>;
    fn engine(&self, model_id: &str) -> Result<Arc<dyn DecisionEngine>, ProviderError>;
}

#[cfg(test)]
mod tests;
