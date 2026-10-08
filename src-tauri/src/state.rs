//! Shared application state behind the Tauri commands.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use tokio_util::sync::CancellationToken;

use crate::commands::AppError;
use crate::engine::{DecisionEngine, LlamaSidecar, TauriLauncher};
use crate::models::{self, REGISTRY};
use crate::providers::{ModelRef, ProviderRegistry, OPENROUTER_BASE};
use crate::store::Store;

/// Settings key holding the active model id.
pub const ACTIVE_MODEL_KEY: &str = "active_model";

/// Sends a named event with a JSON payload to the webview.
pub type Emitter = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// Where the decision engine comes from.
pub enum EngineSource {
    /// Production: a lazily started `llama-server` for the active model.
    Sidecar(tauri::AppHandle),
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
        Self {
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
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn with_openrouter_base(mut self, base: String) -> Self {
        self.openrouter_base = base;
        self
    }

    /// Rebuilds the provider registry from the stored keys.
    pub async fn reload_providers(&self) -> Result<(), AppError> {
        let reg = ProviderRegistry::load(&self.store, &self.openrouter_base).await?;
        *self.providers.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(reg);
        Ok(())
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
            EngineSource::Sidecar(app) => {
                let path = self.installed_path(model_id)?;
                let sidecar = Arc::new(LlamaSidecar::new(Box::new(TauriLauncher(app.clone())), path));
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
        let missing = || AppError::ModelMissing(format!("Model {model_id} is not installed."));
        let spec = REGISTRY.iter().find(|s| s.id == model_id).ok_or_else(missing)?;
        if !models::installed(&self.models_dir).contains(&spec) {
            return Err(missing());
        }
        models::model_path(spec, &self.models_dir).ok_or_else(missing)
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
    }

    pub fn runs(&self) -> Arc<Mutex<HashMap<i64, CancellationToken>>> {
        self.runs.clone()
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
