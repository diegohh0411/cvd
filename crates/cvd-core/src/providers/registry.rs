use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::openrouter::OPENROUTER;
use super::{kind_of, load_rows};
use super::{
    ChatEndpoint, ModelRef, OpenRouterProvider, Provider, ProviderError, RemoteModel, SubQProvider,
    TypeSafeProvider, SUBQ, TYPESAFE, TYPESAFE_BASE,
};
use crate::engine::DecisionEngine;
use crate::store::{StoreError, TenantStore as Store};

const CACHE_TTL: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProviderModels {
    /// The connection id (`openrouter`, `openrouter~2`): what model refs carry.
    pub provider: String,
    pub label: String,
    pub models: Vec<RemoteModel>,
    pub error: Option<String>,
}

/// The remote providers the user has keys for.
#[derive(Default)]
pub struct ProviderRegistry {
    providers: Vec<Connected>,
    cache: Mutex<HashMap<String, (Instant, Vec<RemoteModel>)>>,
}

struct Connected {
    id: String,
    label: String,
    provider: Arc<dyn Provider>,
}

fn kind_name(kind: &str) -> &str {
    match kind {
        OPENROUTER => "OpenRouter",
        SUBQ => "SubQ",
        TYPESAFE => "TypeSafe",
        _ => kind,
    }
}

fn missing_key(provider: &str) -> ProviderError {
    let provider = kind_of(provider);
    if provider == super::LOCAL_SERVER {
        return ProviderError::Missing(
            "Set up your Local server connection in AI → Models.".into(),
        );
    }
    let name = match provider {
        OPENROUTER => "OpenRouter",
        SUBQ => "SubQ",
        TYPESAFE => "TypeSafe",
        _ => provider,
    };
    ProviderError::Missing(format!("Add your {name} key in AI → Models."))
}

impl ProviderRegistry {
    #[cfg_attr(not(test), allow(dead_code))]
    pub async fn load(
        store: &Store,
        openrouter_base: &str,
        subq_base: &str,
    ) -> Result<Self, StoreError> {
        Self::load_with(store, openrouter_base, subq_base, TYPESAFE_BASE).await
    }

    pub async fn load_with(
        store: &Store,
        openrouter_base: &str,
        subq_base: &str,
        typesafe_base: &str,
    ) -> Result<Self, StoreError> {
        let rows = load_rows(store).await?;
        let mut providers = Vec::new();
        for kind in [OPENROUTER, SUBQ, TYPESAFE] {
            // The bare id first, then extra keys in creation order.
            let ids = std::iter::once(kind.to_string()).chain(
                rows.iter()
                    .filter(|r| r.provider == kind && r.id != kind)
                    .map(|r| r.id.clone()),
            );
            for id in ids {
                let Some(key) = store.get_secret(&id).await? else {
                    continue;
                };
                let provider: Arc<dyn Provider> = match kind {
                    OPENROUTER => Arc::new(OpenRouterProvider::new(openrouter_base.into(), key)),
                    SUBQ => Arc::new(SubQProvider::new(subq_base.into(), key)),
                    _ => Arc::new(TypeSafeProvider::new(typesafe_base.into(), key)),
                };
                debug_assert_eq!(provider.id(), kind);
                let label = rows
                    .iter()
                    .find(|r| r.id == id)
                    .map_or_else(|| kind_name(kind).to_string(), |r| r.label.clone());
                providers.push(Connected {
                    id,
                    label,
                    provider,
                });
            }
        }
        if let Some(base) = store.get_setting(super::LOCAL_URL_KEY).await? {
            providers.insert(
                0,
                Connected {
                    id: super::LOCAL_SERVER.into(),
                    label: "Local server".into(),
                    provider: Arc::new(super::LocalProvider::new(
                        base,
                        store.get_secret(super::LOCAL_SERVER).await?,
                    )),
                },
            );
        }
        Ok(Self {
            providers,
            cache: Mutex::default(),
        })
    }

    fn get(&self, connection: &str) -> Option<&Arc<dyn Provider>> {
        self.providers
            .iter()
            .find(|c| c.id == connection)
            .map(|c| &c.provider)
    }

    /// Why `r` can't be used right now, if it can't.
    pub fn check(&self, r: &ModelRef) -> Result<(), ProviderError> {
        self.get(&r.provider)
            .map(|_| ())
            .ok_or_else(|| missing_key(&r.provider))
    }

    pub fn engine(&self, r: &ModelRef) -> Result<Arc<dyn DecisionEngine>, ProviderError> {
        self.get(&r.provider)
            .ok_or_else(|| missing_key(&r.provider))?
            .engine(&r.id)
    }

    pub fn chat_endpoint(&self, r: &ModelRef) -> Result<ChatEndpoint, ProviderError> {
        self.get(&r.provider)
            .ok_or_else(|| missing_key(&r.provider))?
            .chat_endpoint(&r.id)
    }

    /// Every registered provider's decision models; one failing provider never hides the others.
    pub async fn remote_models(&self, refresh: bool) -> Vec<ProviderModels> {
        self.collect(refresh, false).await
    }

    /// Every registered provider's tool-capable chat models.
    pub async fn remote_chat_models(&self, refresh: bool) -> Vec<ProviderModels> {
        self.collect(refresh, true).await
    }

    async fn collect(&self, refresh: bool, chat: bool) -> Vec<ProviderModels> {
        let mut out = Vec::new();
        for c in &self.providers {
            let (id, p) = (c.id.clone(), &c.provider);
            let cache_key = if chat {
                format!("{id}#chat")
            } else {
                id.clone()
            };
            let cached = self
                .cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&cache_key)
                .filter(|(at, _)| !refresh && at.elapsed() < CACHE_TTL)
                .map(|(_, m)| m.clone());
            let result = match cached {
                Some(models) => Ok(models),
                None if chat => p.list_chat_models().await,
                None => p.list_models().await,
            };
            out.push(match result {
                Ok(models) => {
                    self.cache
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(cache_key, (Instant::now(), models.clone()));
                    ProviderModels {
                        provider: id,
                        label: c.label.clone(),
                        models,
                        error: None,
                    }
                }
                Err(e) => ProviderModels {
                    provider: id,
                    label: c.label.clone(),
                    models: vec![],
                    error: Some(e.to_string()),
                },
            });
        }
        out
    }
}
