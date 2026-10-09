//! Web search backends for the company research agent.
mod exa;

use async_trait::async_trait;

use crate::providers::ProviderError;

pub use exa::{ExaSearch, EXA, EXA_BASE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[async_trait]
pub trait SearchProvider: Send + Sync {
    #[allow(dead_code)]
    fn id(&self) -> &'static str;
    async fn search(&self, query: &str, count: u8) -> Result<Vec<SearchHit>, ProviderError>;
}

#[cfg(test)]
mod tests;
