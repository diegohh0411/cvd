//! Shared application state behind the Tauri commands.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio_util::sync::CancellationToken;

use crate::commands::AppError;
use crate::engine::{DecisionEngine, LlamaSidecar, TauriLauncher};
use crate::models::{self, REGISTRY};
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
    emit: Emitter,
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
            emit,
        }
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
        let built = match &self.source {
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

    /// Drops the cached engine (and kills its sidecar).
    pub fn reset_engine(&self) {
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

pub fn remove_run(runs: &Mutex<HashMap<i64, CancellationToken>>, run_id: i64) {
    lock(runs).remove(&run_id);
}
