use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::openrouter::OPENROUTER;
use super::{ModelRef, OpenRouterProvider, Provider, ProviderError, RemoteModel};
use crate::engine::DecisionEngine;
use crate::store::{Store, StoreError};

const CACHE_TTL: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProviderModels {
    pub provider: String,
    pub models: Vec<RemoteModel>,
    pub error: Option<String>,
}

/// The remote providers the user has keys for.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: Vec<Arc<dyn Provider>>,
    cache: Mutex<HashMap<String, (Instant, Vec<RemoteModel>)>>,
}

fn missing_key(provider: &str) -> ProviderError {
    let name = if provider == OPENROUTER { "OpenRouter" } else { provider };
    ProviderError::Missing(format!("Add your {name} key in Settings."))
}

impl ProviderRegistry {
    pub async fn load(store: &Store, openrouter_base: &str) -> Result<Self, StoreError> {
        let mut providers: Vec<Arc<dyn Provider>> = Vec::new();
        if let Some(key) = store.get_secret(OPENROUTER).await? {
            providers.push(Arc::new(OpenRouterProvider::new(openrouter_base.into(), key)));
        }
        Ok(Self { providers, cache: Mutex::default() })
    }

    fn get(&self, provider: &str) -> Option<&Arc<dyn Provider>> {
        self.providers.iter().find(|p| p.id() == provider)
    }

    /// Why `r` can't be used right now, if it can't.
    pub fn check(&self, r: &ModelRef) -> Result<(), ProviderError> {
        self.get(&r.provider).map(|_| ()).ok_or_else(|| missing_key(&r.provider))
    }

    pub fn engine(&self, r: &ModelRef) -> Result<Arc<dyn DecisionEngine>, ProviderError> {
        self.get(&r.provider).ok_or_else(|| missing_key(&r.provider))?.engine(&r.id)
    }

    /// Every registered provider's model list; one failing provider never hides the others.
    pub async fn remote_models(&self, refresh: bool) -> Vec<ProviderModels> {
        let mut out = Vec::new();
        for p in &self.providers {
            let id = p.id().to_string();
            let cached = self
                .cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&id)
                .filter(|(at, _)| !refresh && at.elapsed() < CACHE_TTL)
                .map(|(_, m)| m.clone());
            let result = match cached {
                Some(models) => Ok(models),
                None => p.list_models().await,
            };
            out.push(match result {
                Ok(models) => {
                    self.cache.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), (Instant::now(), models.clone()));
                    ProviderModels { provider: id, models, error: None }
                }
                Err(e) => ProviderModels { provider: id, models: vec![], error: Some(e.to_string()) },
            });
        }
        out
    }
}
