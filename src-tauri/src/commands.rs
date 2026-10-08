//! Command logic as plain async functions over `&AppState` (testable without a
//! Tauri runtime); `ipc` holds the thin `#[tauri::command]` wrappers.

use base64::Engine as _;
use serde::Serialize;
use serde_json::json;

use crate::engine::EngineError;
use crate::matcher::{self, MatchConfig, MatchError, Ranked};
use crate::models::{self, ModelError, ModelSpec, REGISTRY};
use crate::render::fit::{self, Override, Selection};
use crate::render::{self, Compiled, RenderError, StyleStep};
use crate::state::AppState;
#[cfg(test)]
use crate::state::{EngineSource, ACTIVE_MODEL_KEY};
use crate::store::types::*;
use crate::store::StoreError;

pub const MATCH_EVENT: &str = "match://progress";
pub const DOWNLOAD_EVENT: &str = "model://download";
const TEMPLATE: &str = "Classic ATS";
const DOWNLOAD_ATTEMPTS: usize = 4;
const EMPTY_LIBRARY: &str = "Add at least one entry to your library first.";

/// Serialized as `{ kind, message }`.
#[derive(Debug, Clone, PartialEq, thiserror::Error, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum AppError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    ModelMissing(String),
    #[error("{0}")]
    EngineUnavailable(String),
    #[error("{0}")]
    ContextOverflow(String),
    #[error("{0}")]
    Render(String),
    #[error("{0}")]
    Io(String),
}

impl AppError {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NotFound",
            Self::ModelMissing(_) => "ModelMissing",
            Self::EngineUnavailable(_) => "EngineUnavailable",
            Self::ContextOverflow(_) => "ContextOverflow",
            Self::Render(_) => "Render",
            Self::Io(_) => "Io",
        }
    }
}

impl From<StoreError> for AppError {
    fn from(e: StoreError) -> Self {
        match &e {
            StoreError::Db(sqlx::Error::RowNotFound) => Self::NotFound("Not found.".into()),
            StoreError::Invalid(m) if m.starts_with("no ") => Self::NotFound(m.clone()),
            _ => Self::Io(e.to_string()),
        }
    }
}

impl From<EngineError> for AppError {
    fn from(e: EngineError) -> Self {
        match e {
            EngineError::ContextOverflow => Self::ContextOverflow(e.to_string()),
            _ => Self::EngineUnavailable(e.to_string()),
        }
    }
}

impl From<MatchError> for AppError {
    fn from(e: MatchError) -> Self {
        match e {
            MatchError::Engine(e) => e.into(),
            MatchError::Store(e) => e.into(),
            MatchError::JdNotFound(_) => Self::NotFound(e.to_string()),
            MatchError::Cancelled { .. } => Self::Io(e.to_string()),
        }
    }
}

impl From<RenderError> for AppError {
    fn from(e: RenderError) -> Self {
        Self::Render(e.to_string())
    }
}

impl From<ModelError> for AppError {
    fn from(e: ModelError) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

fn join_err(e: tokio::task::JoinError) -> AppError {
    AppError::Io(format!("background task failed: {e}"))
}

type Res<T> = Result<T, AppError>;

// ---- profile & library ----

pub async fn get_profile(st: &AppState) -> Res<Profile> {
    Ok(st.store.get_profile().await?.unwrap_or_default())
}

pub async fn save_profile(st: &AppState, profile: Profile) -> Res<()> {
    Ok(st.store.upsert_profile(&profile).await?)
}

pub async fn list_entries(st: &AppState, include_archived: bool) -> Res<Vec<Entry>> {
    Ok(st.store.list_entries(include_archived).await?)
}

pub async fn save_entry(st: &AppState, input: EntryInput) -> Res<i64> {
    Ok(st.store.upsert_entry(&input).await?)
}

pub async fn delete_entry(st: &AppState, id: i64) -> Res<()> {
    Ok(st.store.delete_entry(id).await?)
}

pub async fn list_skills(st: &AppState) -> Res<Vec<Skill>> {
    Ok(st.store.list_skills().await?)
}

// ---- jobs ----

pub async fn create_job(st: &AppState, title: &str, company: &str, raw_text: &str) -> Res<i64> {
    Ok(st.store.create_jd(title, company, raw_text).await?)
}

pub async fn list_jobs(st: &AppState) -> Res<Vec<JobDescription>> {
    Ok(st.store.list_jds().await?)
}

pub async fn get_job(st: &AppState, id: i64) -> Res<JobDescription> {
    st.store
        .get_jd(id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Job description {id} not found.")))
}

// ---- matching ----

/// Creates the run, then scores it in a background task so the run id is
/// returned at once; progress streams on `match://progress`.
pub async fn start_match(st: &AppState, jd_id: i64) -> Res<i64> {
    get_job(st, jd_id).await?;
    if st.store.list_entries(false).await?.is_empty() {
        return Err(AppError::NotFound(EMPTY_LIBRARY.into()));
    }
    let model_id = st.active_model().await?;
    st.engine_for(&model_id)?;
    let run_id = st.store.create_run(jd_id, &model_id).await?;
    spawn_run(st, run_id, &model_id)?;
    Ok(run_id)
}

/// Continues a cancelled or failed run in the background.
pub async fn resume_match(st: &AppState, run_id: i64) -> Res<()> {
    let run = st.store.get_run(run_id).await?;
    spawn_run(st, run_id, &run.model_id)
}

pub fn cancel_match(st: &AppState, run_id: i64) -> Res<()> {
    if st.cancel_run(run_id) {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("Match run {run_id} is not running.")))
    }
}

/// Scores `run_id` in a spawned task. A freshly created run has no scores,
/// so `matcher::resume_match` scores it end to end.
fn spawn_run(st: &AppState, run_id: i64, model_id: &str) -> Res<()> {
    let engine = st.engine_for(model_id)?;
    let token = st
        .register_run(run_id)
        .ok_or_else(|| AppError::Io(format!("Match run {run_id} is already running.")))?;
    let (store, emit, runs) = (st.store.clone(), st.emitter(), st.runs());
    tokio::spawn(async move {
        let progress_emit = emit.clone();
        let outcome = matcher::resume_match(
            &store,
            &*engine,
            run_id,
            MatchConfig::default(),
            token,
            move |p| progress_emit(MATCH_EVENT, json!({ "run_id": run_id, "event": p })),
        )
        .await;
        match outcome {
            Ok(_) => {}
            Err(MatchError::Cancelled { .. }) => {
                emit(MATCH_EVENT, json!({ "run_id": run_id, "event": { "stage": "cancelled" } }))
            }
            Err(e) => emit(
                MATCH_EVENT,
                json!({ "run_id": run_id, "event": { "stage": "failed", "error": AppError::from(e) } }),
            ),
        }
        crate::state::remove_run(&runs, run_id);
    });
    Ok(())
}

pub async fn get_ranked(st: &AppState, run_id: i64) -> Res<Ranked> {
    st.store.get_run(run_id).await?;
    let scores = st.store.scores_for_run(run_id).await?;
    let entries = st.store.list_entries(false).await?;
    let skills = st.store.list_skills().await?;
    Ok(matcher::rank(&scores, &entries, &skills))
}

// ---- CV ----

#[derive(Debug, Clone, Serialize)]
pub struct FitOutput {
    pub selection: Selection,
    pub pages: usize,
    pub fill: f32,
    pub overflow: bool,
    pub pdf_base64: String,
}

pub async fn fit_cv(st: &AppState, run_id: i64, budget: u8, overrides: Vec<Override>) -> Res<FitOutput> {
    let ranked = get_ranked(st, run_id).await?;
    let profile = get_profile(st).await?;
    let floor = MatchConfig::default().score_floor;
    let r = tokio::task::spawn_blocking(move || fit::fit(&ranked, &profile, budget, floor, &overrides))
        .await
        .map_err(join_err)??;
    Ok(FitOutput {
        selection: r.selection,
        pages: r.compiled.pages,
        fill: r.compiled.fill,
        overflow: r.overflow,
        pdf_base64: base64::engine::general_purpose::STANDARD.encode(&r.compiled.pdf),
    })
}

/// Compiles `selection` at the loosest style that needs no more pages than
/// the tightest one, writes the PDF to `path` and records the CV.
pub async fn export_cv(st: &AppState, run_id: i64, selection: Selection, path: String) -> Res<i64> {
    let run = st.store.get_run(run_id).await?;
    let entries = st.store.list_entries(true).await?;
    let profile = get_profile(st).await?;
    let sel = selection.clone();
    let out = path.clone();
    let compiled = tokio::task::spawn_blocking(move || -> Res<Compiled> {
        let doc = |s| fit::selection_doc(&entries, &profile, &sel, StyleStep(s));
        let tight = render::compile(&doc(2))?;
        let mut best = tight;
        for s in [1, 0] {
            let c = render::compile(&doc(s))?;
            if c.pages <= best.pages {
                best = c;
            }
        }
        std::fs::write(&out, &best.pdf)?;
        Ok(best)
    })
    .await
    .map_err(join_err)??;
    let selection_json = serde_json::to_string(&selection).map_err(|e| AppError::Io(e.to_string()))?;
    Ok(st
        .store
        .save_cv(&CvInput {
            jd_id: run.jd_id,
            run_id: Some(run_id),
            template: TEMPLATE.into(),
            page_budget: compiled.pages as i64,
            selection_json,
            pdf_path: Some(path),
        })
        .await?)
}

// ---- models ----

#[derive(Debug, Serialize)]
pub struct ModelInfo {
    pub spec: &'static ModelSpec,
    pub installed: bool,
}

pub fn list_models(st: &AppState) -> Vec<ModelInfo> {
    let installed = models::installed(&st.models_dir);
    REGISTRY
        .iter()
        .map(|spec| ModelInfo { spec, installed: installed.contains(&spec) })
        .collect()
}

/// Downloads a model unless it is already installed, retrying network errors
/// (hf-hub resumes partial files, so a retry continues where it stopped).
pub async fn download_model(st: &AppState, id: &str) -> Res<()> {
    let spec = REGISTRY
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Unknown model {id}.")))?;
    if models::installed(&st.models_dir).contains(&spec) {
        return Ok(());
    }
    let mut last = None;
    for _ in 0..DOWNLOAD_ATTEMPTS {
        let emit = st.emitter();
        let on_progress = move |done: u64, total: u64| {
            emit(DOWNLOAD_EVENT, json!({ "id": spec.id, "done": done, "total": total }))
        };
        match models::download(spec, &st.models_dir, on_progress).await {
            Ok(_) => return Ok(()),
            Err(e @ ModelError::Network(_)) => last = Some(e),
            Err(e) => return Err(e.into()),
        }
    }
    Err(last.expect("at least one attempt").into())
}

pub async fn set_active_model(st: &AppState, id: &str) -> Res<()> {
    st.installed_path(id)?;
    st.store.set_setting(crate::state::ACTIVE_MODEL_KEY, id).await?;
    st.reset_engine();
    Ok(())
}

pub mod ipc {
    //! `#[tauri::command]` wrappers; names are the IPC contract.
    use tauri::State;

    use super::*;

    type S<'a> = State<'a, AppState>;

    #[tauri::command]
    pub async fn get_profile(st: S<'_>) -> Res<Profile> {
        super::get_profile(&st).await
    }
    #[tauri::command]
    pub async fn save_profile(st: S<'_>, profile: Profile) -> Res<()> {
        super::save_profile(&st, profile).await
    }
    #[tauri::command]
    pub async fn list_entries(st: S<'_>, include_archived: bool) -> Res<Vec<Entry>> {
        super::list_entries(&st, include_archived).await
    }
    #[tauri::command]
    pub async fn save_entry(st: S<'_>, input: EntryInput) -> Res<i64> {
        super::save_entry(&st, input).await
    }
    #[tauri::command]
    pub async fn delete_entry(st: S<'_>, id: i64) -> Res<()> {
        super::delete_entry(&st, id).await
    }
    #[tauri::command]
    pub async fn list_skills(st: S<'_>) -> Res<Vec<Skill>> {
        super::list_skills(&st).await
    }
    #[tauri::command]
    pub async fn create_job(st: S<'_>, title: String, company: String, raw_text: String) -> Res<i64> {
        super::create_job(&st, &title, &company, &raw_text).await
    }
    #[tauri::command]
    pub async fn list_jobs(st: S<'_>) -> Res<Vec<JobDescription>> {
        super::list_jobs(&st).await
    }
    #[tauri::command]
    pub async fn get_job(st: S<'_>, id: i64) -> Res<JobDescription> {
        super::get_job(&st, id).await
    }
    #[tauri::command]
    pub async fn start_match(st: S<'_>, jd_id: i64) -> Res<i64> {
        super::start_match(&st, jd_id).await
    }
    #[tauri::command]
    pub fn cancel_match(st: S<'_>, run_id: i64) -> Res<()> {
        super::cancel_match(&st, run_id)
    }
    #[tauri::command]
    pub async fn resume_match(st: S<'_>, run_id: i64) -> Res<()> {
        super::resume_match(&st, run_id).await
    }
    #[tauri::command]
    pub async fn get_ranked(st: S<'_>, run_id: i64) -> Res<Ranked> {
        super::get_ranked(&st, run_id).await
    }
    #[tauri::command]
    pub async fn fit_cv(st: S<'_>, run_id: i64, budget: u8, overrides: Vec<Override>) -> Res<FitOutput> {
        super::fit_cv(&st, run_id, budget, overrides).await
    }
    #[tauri::command]
    pub async fn export_cv(st: S<'_>, run_id: i64, selection: Selection, path: String) -> Res<i64> {
        super::export_cv(&st, run_id, selection, path).await
    }
    #[tauri::command]
    pub fn system_ram_gb() -> u32 {
        crate::models::system_ram_gb()
    }
    #[tauri::command]
    pub fn list_models(st: S<'_>) -> Vec<ModelInfo> {
        super::list_models(&st)
    }
    #[tauri::command]
    pub async fn download_model(st: S<'_>, id: String) -> Res<()> {
        super::download_model(&st, &id).await
    }
    #[tauri::command]
    pub async fn set_active_model(st: S<'_>, id: String) -> Res<()> {
        super::set_active_model(&st, &id).await
    }
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
