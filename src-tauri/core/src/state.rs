//! Shared application state behind the commands.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use tokio_util::sync::CancellationToken;

use crate::commands::AppError;
use crate::company::agent::ResearchError;
use crate::company::coordinator::{ResearchCoordinator, ResearchFn};
use crate::engine::{DecisionEngine, Launcher, LlamaSidecar};
use crate::models::{self, ModelKind, REGISTRY};
use crate::providers::{ChatEndpoint, ModelRef, ProviderError, ProviderRegistry, OPENROUTER_BASE, SUBQ_BASE, TYPESAFE_BASE};
use crate::search::{ExaSearch, SearchProvider, EXA, EXA_BASE};
use crate::store::Store;

/// Settings key holding the active model id.
pub const ACTIVE_MODEL_KEY: &str = "active_model";
/// Settings key holding the company research (chat) model id, same encoding as `ACTIVE_MODEL_KEY`.
pub const RESEARCH_MODEL_KEY: &str = "research_model";

/// Sends a named event with a JSON payload to the webview.
pub type Emitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// Builds a fresh launcher for a `llama-server` process (one per sidecar).
pub type LauncherFactory = Arc<dyn Fn() -> Box<dyn Launcher> + Send + Sync>;

/// Where the decision engine comes from.
pub enum EngineSource {
    /// Production: a lazily started `llama-server` for the active model.
    Sidecar(LauncherFactory),
    /// Tests: a fixed engine (e.g. `FakeDecisionEngine`); no model file needed.
    #[cfg_attr(not(test), allow(dead_code))]
    Fixed(Arc<dyn DecisionEngine>),
}

struct Built {
    model_id: String,
    engine: Arc<dyn DecisionEngine>,
    sidecar: Option<Arc<LlamaSidecar>>,
}

pub struct AppState {
    pub store: Store,
    pub models_dir: PathBuf,
    source: EngineSource,
    engine: Mutex<Option<Built>>,
    runs: Arc<Mutex<HashMap<i64, CancellationToken>>>,
    downloads: Mutex<HashMap<String, CancellationToken>>,
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
    /// The local chat-model server for research, kept apart from the decision
    /// sidecar so switching System One models never stops it.
    research: ResearchSlot,
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
    /// `none` (no active model), `missing` (active model not installed),
    /// `idle` (starts on the next match), `ready` (server running) or `error`.
    pub state: &'static str,
    pub error: Option<String>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn new(store: Store, models_dir: PathBuf, source: EngineSource, emit: Emitter) -> Self {
        let coordinator = Arc::new(ResearchCoordinator::new(1, emit.clone()));
        Self {
            coordinator,
            store,
            models_dir,
            source,
            engine: Mutex::new(None),
            runs: Arc::default(),
            downloads: Mutex::default(),
            engine_error: Arc::default(),
            emit,
            providers: RwLock::default(),
            openrouter_base: OPENROUTER_BASE.into(),
            subq_base: SUBQ_BASE.into(),
            typesafe_base: TYPESAFE_BASE.into(),
            search: RwLock::default(),
            exa_base: EXA_BASE.into(),
            research: ResearchSlot::default(),
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
        let reg = ProviderRegistry::load_with(&self.store, &self.openrouter_base, &self.subq_base, &self.typesafe_base).await?;
        *self.providers.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(reg);
        let search = self
            .store
            .get_secret(EXA)
            .await?
            .map(|k| Arc::new(ExaSearch::new(self.exa_base.clone(), k)) as Arc<dyn SearchProvider>);
        *self.search.write().unwrap_or_else(|e| e.into_inner()) = search;
        Ok(())
    }

    pub async fn research_model(&self) -> Result<Option<String>, AppError> {
        Ok(self.store.get_setting(RESEARCH_MODEL_KEY).await?)
    }

    /// Where the research agent sends chat requests. A local model starts (or
    /// reuses) its own llama-server.
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
            launcher: match &self.source {
                EngineSource::Sidecar(launch) => Some(launch.clone()),
                EngineSource::Fixed(_) => None,
            },
            slot: self.research.clone(),
            models_dir: self.models_dir.clone(),
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
        lock(&self.skips).remove(&run_id).map(|t| t.cancel()).is_some()
    }

    pub fn skips(&self) -> Skips {
        self.skips.clone()
    }

    /// Stops the local research server, if any (e.g. after the research model changes).
    pub fn reset_research(&self) {
        if let Some((_, s)) = lock(&self.research).take() {
            s.shutdown();
        }
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
        self.providers.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn emitter(&self) -> Emitter {
        self.emit.clone()
    }

    /// The active model id, or `ModelMissing` when none is set.
    pub async fn active_model(&self) -> Result<String, AppError> {
        self.store
            .get_setting(ACTIVE_MODEL_KEY)
            .await?
            .ok_or_else(|| AppError::ModelMissing("No decision model is active. Download one first.".into()))
    }

    /// The engine for `model_id`, built on first use. Switching models shuts
    /// the previous sidecar down.
    pub fn engine_for(&self, model_id: &str) -> Result<Arc<dyn DecisionEngine>, AppError> {
        let mut slot = lock(&self.engine);
        if let Some(b) = slot.as_ref().filter(|b| b.model_id == model_id) {
            return Ok(b.engine.clone());
        }
        let r = ModelRef::parse(model_id);
        let built = match &self.source {
            _ if !r.is_local() => {
                Built { model_id: model_id.into(), engine: self.providers().engine(&r)?, sidecar: None }
            }
            EngineSource::Fixed(e) => Built { model_id: model_id.into(), engine: e.clone(), sidecar: None },
            EngineSource::Sidecar(launch) => {
                let path = self.installed_path(model_id)?;
                let sidecar = Arc::new(LlamaSidecar::new(launch(), path));
                Built { model_id: model_id.into(), engine: sidecar.clone(), sidecar: Some(sidecar) }
            }
        };
        let engine = built.engine.clone();
        if let Some(old) = slot.replace(built).and_then(|b| b.sidecar) {
            old.shutdown();
        }
        Ok(engine)
    }

    /// Path of an installed model file, or `ModelMissing`.
    pub fn installed_path(&self, model_id: &str) -> Result<PathBuf, AppError> {
        installed_path(&self.models_dir, model_id)
    }

    pub async fn engine_status(&self) -> Result<EngineStatus, AppError> {
        let model_id = self.store.get_setting(ACTIVE_MODEL_KEY).await?;
        let error = lock(&self.engine_error).clone();
        let remote_problem = model_id
            .as_deref()
            .map(ModelRef::parse)
            .filter(|r| !r.is_local())
            .and_then(|r| self.providers().check(&r).err());
        let error = error.or(remote_problem.map(|e| e.to_string()));
        let state = match &model_id {
            None => "none",
            Some(_) if error.is_some() => "error",
            Some(id)
                if ModelRef::parse(id).is_local()
                    && self.installed_path(id).is_err()
                    && matches!(self.source, EngineSource::Sidecar(_)) =>
            {
                "missing"
            }
            Some(id) => {
                let slot = lock(&self.engine);
                let running = slot
                    .as_ref()
                    .filter(|b| &b.model_id == id)
                    .and_then(|b| b.sidecar.as_ref())
                    .is_some_and(|s| s.is_running());
                if running || matches!(self.source, EngineSource::Fixed(_)) && slot.is_some() {
                    "ready"
                } else {
                    "idle"
                }
            }
        };
        Ok(EngineStatus { model_id, state, error })
    }

    pub fn engine_error_slot(&self) -> Arc<Mutex<Option<String>>> {
        self.engine_error.clone()
    }

    /// Registers a cancellable download; `None` if `id` is already downloading.
    pub fn register_download(&self, id: &str) -> Option<CancellationToken> {
        let mut d = lock(&self.downloads);
        if d.contains_key(id) {
            return None;
        }
        let token = CancellationToken::new();
        d.insert(id.to_string(), token.clone());
        Some(token)
    }

    pub fn finish_download(&self, id: &str) {
        lock(&self.downloads).remove(id);
    }

    pub fn cancel_download(&self, id: &str) -> bool {
        lock(&self.downloads).get(id).map(|t| t.cancel()).is_some()
    }

    /// Drops the cached engine (and kills its sidecar).
    pub fn reset_engine(&self) {
        *lock(&self.engine_error) = None;
        if let Some(s) = lock(&self.engine).take().and_then(|b| b.sidecar) {
            s.shutdown();
        }
    }

    /// Kills the sidecar if one was ever built; a no-op otherwise.
    pub fn shutdown(&self) {
        for token in lock(&self.runs).values() {
            token.cancel();
        }
        self.reset_engine();
        self.reset_research();
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

fn installed_path(models_dir: &Path, model_id: &str) -> Result<PathBuf, AppError> {
    let missing = || AppError::ModelMissing(format!("Model {model_id} is not installed."));
    let spec = REGISTRY.iter().find(|s| s.id == model_id).ok_or_else(missing)?;
    if !models::installed(models_dir).contains(&spec) {
        return Err(missing());
    }
    models::model_path(spec, models_dir).ok_or_else(missing)
}

/// Runs currently waiting on company research, by run id.
pub type Skips = Arc<Mutex<HashMap<i64, CancellationToken>>>;

/// The local research server, keyed by model id.
type ResearchSlot = Arc<Mutex<Option<(String, Arc<LlamaSidecar>)>>>;

#[derive(Clone)]
pub struct ResearchEnv {
    pub store: Store,
    providers: Arc<ProviderRegistry>,
    search: Option<Arc<dyn SearchProvider>>,
    launcher: Option<LauncherFactory>,
    slot: ResearchSlot,
    models_dir: PathBuf,
    override_fn: Option<ResearchFn>,
}

impl ResearchEnv {
    pub async fn endpoint(&self) -> Result<ChatEndpoint, AppError> {
        let id = self
            .store
            .get_setting(RESEARCH_MODEL_KEY)
            .await?
            .ok_or_else(|| AppError::ModelMissing("Choose a Company research model in Settings.".into()))?;
        let r = ModelRef::parse(&id);
        if !r.is_local() {
            return Ok(self.providers.chat_endpoint(&r)?);
        }
        let Some(launch) = &self.launcher else {
            return Err(AppError::EngineUnavailable("Local research models need the app runtime.".into()));
        };
        let sidecar = {
            let mut slot = lock(&self.slot);
            match slot.as_ref().filter(|(m, _)| *m == id) {
                Some((_, s)) => s.clone(),
                None => {
                    let path = installed_path(&self.models_dir, &id)?;
                    let s = Arc::new(
                        LlamaSidecar::new(launch(), path).with_kind(ModelKind::Chat),
                    );
                    if let Some((_, old)) = slot.replace((id.clone(), s.clone())) {
                        old.shutdown();
                    }
                    s
                }
            }
        };
        let port = sidecar.ensure_started().await?;
        Ok(ChatEndpoint { base_url: format!("http://127.0.0.1:{port}/v1"), api_key: None, model: r.id })
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
                    .ok_or_else(|| unavailable("Choose a Company research model in Settings.".into()))?;
                let search = env.search.clone().ok_or_else(|| unavailable("Add your Exa key in Settings.".into()))?;
                let ep = env.endpoint().await.map_err(|e| unavailable(e.to_string()))?;
                let out = crate::company::agent::research(&ep, search, input, cancel, on_step).await?;
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
            std::env::temp_dir().join("cvd-test-models-scan"),
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
