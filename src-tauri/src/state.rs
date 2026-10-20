//! Shared application state behind the Tauri commands.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use tokio_util::sync::CancellationToken;

use crate::commands::AppError;
use crate::company::agent::ResearchError;
use crate::company::coordinator::{ResearchCoordinator, ResearchFn};
use crate::engine::DecisionEngine;
use crate::providers::{
    ChatEndpoint, ModelRef, ProviderError, ProviderRegistry, OPENROUTER_BASE, SUBQ_BASE,
    TYPESAFE_BASE,
};
use crate::search::{ExaSearch, SearchProvider, EXA, EXA_BASE};
use crate::store::Store;

/// Settings key holding the active model id.
pub const ACTIVE_MODEL_KEY: &str = "active_model";
/// Settings key holding the company research (chat) model id, same encoding as `ACTIVE_MODEL_KEY`.
pub const RESEARCH_MODEL_KEY: &str = "research_model";

/// Sends a named event with a JSON payload to the webview.
pub type Emitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// Where the decision engine comes from.
pub enum EngineSource {
    /// Production: HTTP connections owned by the user.
    Http,
    /// Tests: a fixed engine (e.g. `FakeDecisionEngine`); no model file needed.
    #[cfg_attr(not(test), allow(dead_code))]
    Fixed(Arc<dyn DecisionEngine>),
}

struct Built {
    model_id: String,
    engine: Arc<dyn DecisionEngine>,
}

pub struct AppState {
    pub store: Store,
    source: EngineSource,
    engine: Mutex<Option<Built>>,
    runs: Arc<Mutex<HashMap<i64, CancellationToken>>>,
    /// Last engine failure, shown in the UI until the engine is reset or a run succeeds.
    engine_error: Arc<Mutex<Option<String>>>,
    emit: Emitter,
    /// Remote providers with keys; rebuilt by `reload_providers`.
    providers: RwLock<Arc<ProviderRegistry>>,
    pub openrouter_base: String,
    pub subq_base: String,
    pub typesafe_base: String,
    /// Web search for company research; `None` until an Exa key is stored.
    search: RwLock<Option<Arc<dyn SearchProvider>>>,
    pub exa_base: String,
    /// Tests: replaces the real research agent.
    research_fn: Option<ResearchFn>,
    /// Per-run signal to stop waiting for company research (see `skip_research`).
    skips: Skips,
    /// Company research runs (single-flight per company, one at a time).
    pub coordinator: Arc<ResearchCoordinator>,
    /// Set while a Gmail scan runs; see `begin_scan`.
    scan_running: Arc<AtomicBool>,
}

/// Holds the single Gmail scan slot; the slot frees when the guard drops.
pub struct ScanGuard(Arc<AtomicBool>);

impl Drop for ScanGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// What the sidebar footer shows about the decision engine.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EngineStatus {
    pub model_id: Option<String>,
    /// `none` (no model), `idle` (configured), `ready` (engine built), or `error`.
    pub state: &'static str,
    pub error: Option<String>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn new(store: Store, source: EngineSource, emit: Emitter) -> Self {
        let coordinator = Arc::new(ResearchCoordinator::new(1, emit.clone()));
        Self {
            coordinator,
            store,
            source,
            engine: Mutex::new(None),
            runs: Arc::default(),
            engine_error: Arc::default(),
            emit,
            providers: RwLock::default(),
            openrouter_base: OPENROUTER_BASE.into(),
            subq_base: SUBQ_BASE.into(),
            typesafe_base: TYPESAFE_BASE.into(),
            search: RwLock::default(),
            exa_base: EXA_BASE.into(),
            research_fn: None,
            skips: Skips::default(),
            scan_running: Arc::default(),
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn with_openrouter_base(mut self, base: String) -> Self {
        self.openrouter_base = base;
        self
    }

    pub fn with_subq_base(mut self, base: String) -> Self {
        self.subq_base = base;
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn with_typesafe_base(mut self, base: String) -> Self {
        self.typesafe_base = base;
        self
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn with_exa_base(mut self, base: String) -> Self {
        self.exa_base = base;
        self
    }

    /// Rebuilds the provider registry and search backend from the stored keys.
    pub async fn reload_providers(&self) -> Result<(), AppError> {
        let reg = ProviderRegistry::load_with(
            &self.store,
            &self.openrouter_base,
            &self.subq_base,
            &self.typesafe_base,
        )
        .await?;
        *self.providers.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(reg);
        let search =
            self.store.get_secret(EXA).await?.map(|k| {
                Arc::new(ExaSearch::new(self.exa_base.clone(), k)) as Arc<dyn SearchProvider>
            });
        *self.search.write().unwrap_or_else(|e| e.into_inner()) = search;
        Ok(())
    }

    pub async fn research_model(&self) -> Result<Option<String>, AppError> {
        Ok(self.store.get_setting(RESEARCH_MODEL_KEY).await?)
    }

    /// Where the research agent sends chat requests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub async fn research_endpoint(&self) -> Result<ChatEndpoint, AppError> {
        self.research_env().endpoint().await
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn with_research_fn(mut self, f: ResearchFn) -> Self {
        self.research_fn = Some(f);
        self
    }

    /// Everything company research needs, detached from `self` so spawned tasks can own it.
    pub fn research_env(&self) -> ResearchEnv {
        ResearchEnv {
            store: self.store.clone(),
            providers: self.providers(),
            search: self.search().ok(),
            override_fn: self.research_fn.clone(),
        }
    }

    /// Registers a skip signal for a run that is about to wait on research.
    pub fn register_skip(&self, run_id: i64) -> CancellationToken {
        let t = CancellationToken::new();
        lock(&self.skips).insert(run_id, t.clone());
        t
    }

    /// Fires the skip signal of a run waiting on research; false when none waits.
    pub fn skip(&self, run_id: i64) -> bool {
        lock(&self.skips)
            .remove(&run_id)
            .map(|t| t.cancel())
            .is_some()
    }

    pub fn skips(&self) -> Skips {
        self.skips.clone()
    }

    /// The web search backend, or why there is none.
    pub fn search(&self) -> Result<Arc<dyn SearchProvider>, ProviderError> {
        self.search
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or_else(|| ProviderError::Missing("Add your Exa key in Settings.".into()))
    }

    pub fn providers(&self) -> Arc<ProviderRegistry> {
        self.providers
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn emitter(&self) -> Emitter {
        self.emit.clone()
    }

    /// The active model id, or `ModelMissing` when none is set.
    pub async fn active_model(&self) -> Result<String, AppError> {
        self.store
            .get_setting(ACTIVE_MODEL_KEY)
            .await?
            .ok_or_else(|| AppError::ModelMissing("Choose a Fast Model in AI Models.".into()))
    }

    /// The HTTP engine for `model_id`, cached until the connection changes.
    pub fn engine_for(&self, model_id: &str) -> Result<Arc<dyn DecisionEngine>, AppError> {
        let mut slot = lock(&self.engine);
        if let Some(b) = slot.as_ref().filter(|b| b.model_id == model_id) {
            return Ok(b.engine.clone());
        }
        let engine = match &self.source {
            EngineSource::Fixed(e) if ModelRef::parse(model_id).is_local() => e.clone(),
            _ => self.providers().engine(&ModelRef::parse(model_id))?,
        };
        *slot = Some(Built {
            model_id: model_id.into(),
            engine: engine.clone(),
        });
        Ok(engine)
    }

    pub async fn engine_status(&self) -> Result<EngineStatus, AppError> {
        let model_id = self.store.get_setting(ACTIVE_MODEL_KEY).await?;
        let problem = model_id
            .as_deref()
            .map(ModelRef::parse)
            .filter(|r| !r.is_local())
            .and_then(|r| self.providers().check(&r).err());
        let error = lock(&self.engine_error)
            .clone()
            .or(problem.map(|e| e.to_string()));
        let state = if model_id.is_none() {
            "none"
        } else if error.is_some() {
            "error"
        } else if lock(&self.engine).is_some() {
            "ready"
        } else {
            "idle"
        };
        Ok(EngineStatus {
            model_id,
            state,
            error,
        })
    }

    pub fn engine_error_slot(&self) -> Arc<Mutex<Option<String>>> {
        self.engine_error.clone()
    }

    pub fn reset_engine(&self) {
        *lock(&self.engine_error) = None;
        *lock(&self.engine) = None;
    }

    pub fn runs(&self) -> Arc<Mutex<HashMap<i64, CancellationToken>>> {
        self.runs.clone()
    }

    /// Claims the Gmail scan slot; `None` while another scan holds it.
    pub fn begin_scan(&self) -> Option<ScanGuard> {
        self.scan_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| ScanGuard(self.scan_running.clone()))
    }

    pub fn scan_running(&self) -> bool {
        self.scan_running.load(Ordering::SeqCst)
    }

    /// Registers a fresh token for `run_id`; `None` if it is already running.
    pub fn register_run(&self, run_id: i64) -> Option<CancellationToken> {
        let mut runs = lock(&self.runs);
        if runs.contains_key(&run_id) {
            return None;
        }
        let token = CancellationToken::new();
        runs.insert(run_id, token.clone());
        Some(token)
    }

    pub fn cancel_run(&self, run_id: i64) -> bool {
        lock(&self.runs).get(&run_id).map(|t| t.cancel()).is_some()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_running(&self, run_id: i64) -> bool {
        lock(&self.runs).contains_key(&run_id)
    }
}

pub fn set_engine_error(slot: &Mutex<Option<String>>, error: Option<String>) {
    *lock(slot) = error;
}

pub fn remove_run(runs: &Mutex<HashMap<i64, CancellationToken>>, run_id: i64) {
    lock(runs).remove(&run_id);
}

/// Runs currently waiting on company research, by run id.
pub type Skips = Arc<Mutex<HashMap<i64, CancellationToken>>>;

#[derive(Clone)]
pub struct ResearchEnv {
    pub store: Store,
    providers: Arc<ProviderRegistry>,
    search: Option<Arc<dyn SearchProvider>>,
    override_fn: Option<ResearchFn>,
}

impl ResearchEnv {
    pub async fn endpoint(&self) -> Result<ChatEndpoint, AppError> {
        let id = self
            .store
            .get_setting(RESEARCH_MODEL_KEY)
            .await?
            .ok_or_else(|| {
                AppError::ModelMissing("Choose a Company research model in Settings.".into())
            })?;
        Ok(self.providers.chat_endpoint(&ModelRef::parse(&id))?)
    }

    /// The agent run the coordinator executes. Endpoint and search are resolved
    /// lazily, inside the run, so a fresh profile never starts a server; a missing
    /// key or model becomes `ResearchError::Unavailable`.
    pub fn research_fn(&self) -> ResearchFn {
        if let Some(f) = &self.override_fn {
            return f.clone();
        }
        let env = self.clone();
        Arc::new(move |input, cancel, on_step| {
            let env = env.clone();
            Box::pin(async move {
                let unavailable = |m: String| ResearchError::Unavailable(m);
                // Cheapest checks first: no server starts unless research can run.
                let model = env
                    .store
                    .get_setting(RESEARCH_MODEL_KEY)
                    .await
                    .ok()
                    .flatten()
                    .ok_or_else(|| {
                        unavailable("Choose a Company research model in Settings.".into())
                    })?;
                let search = env
                    .search
                    .clone()
                    .ok_or_else(|| unavailable("Add your Exa key in Settings.".into()))?;
                let ep = env
                    .endpoint()
                    .await
                    .map_err(|e| unavailable(e.to_string()))?;
                let out =
                    crate::company::agent::research(&ep, search, input, cancel, on_step).await?;
                Ok((out, model))
            })
        })
    }
}

#[cfg(test)]
mod scan_slot_tests {
    use super::*;
    use crate::engine::FakeDecisionEngine;

    #[tokio::test]
    async fn begin_scan_is_single_flight() {
        let store = Store::open("sqlite::memory:").await.unwrap();
        let st = AppState::new(
            store,
            EngineSource::Fixed(Arc::new(FakeDecisionEngine::new(|_, _, _| {
                crate::engine::Answer::Noul { p: 0.0 }
            }))),
            Arc::new(|_, _| {}),
        );

        let first = st.begin_scan().expect("first claim succeeds");
        assert!(st.scan_running());
        assert!(st.begin_scan().is_none());

        drop(first);
        assert!(!st.scan_running());
        assert!(st.begin_scan().is_some());
    }
}
