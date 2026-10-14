//! Command logic as plain async functions over `&AppState` (testable without a
//! Tauri runtime); `ipc` holds the thin `#[tauri::command]` wrappers.

use base64::Engine as _;
use serde::Serialize;
use serde_json::json;

use crate::company::{self, agent::{ResearchError, ResearchInput, JD_SNIPPET_CHARS}, coordinator::ResearchCoordinator};
use crate::search::{ExaSearch, SearchProvider, EXA};
use crate::state::ResearchEnv;
use crate::store::types::{Company, CompanyProfile};
use crate::store::StoreError;
use tokio_util::sync::CancellationToken;
use crate::providers::{ModelRef, OpenRouterProvider, ProviderModels, SubQProvider, OPENROUTER, SUBQ};
use crate::engine::EngineError;
use crate::matcher::{self, MatchConfig, MatchError, Ranked};
use crate::models::{self, ModelError, ModelKind, ModelSpec, REGISTRY};
use crate::render::fit::{self, Override, Selection};
use crate::render::{self, Compiled, RenderError, StyleStep};
use crate::mail::account::{self, MailAccount};
use crate::mail::imap::{ImapSource, MailSource};
use crate::mail::scan::{self, ScanDeps};
use crate::state::AppState;
use crate::web::{self, FetchPolicy};
#[cfg(test)]
use crate::state::{EngineSource, ACTIVE_MODEL_KEY};
use crate::store::types::*;
use crate::store::{ProcessedEmail, Store};

pub const MATCH_EVENT: &str = "match://progress";
pub const DOWNLOAD_EVENT: &str = "model://download";
pub const ENGINE_EVENT: &str = "engine://status";
/// Settings keys the webview may read and write (UI preferences only).
const UI_SETTING_PREFIX: &str = "ui.";
const TEMPLATE: &str = "Classic ATS";
const DOWNLOAD_ATTEMPTS: usize = 4;
pub const DOWNLOAD_PAUSED: &str = "Download paused.";
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
    #[error("{0}")]
    ProviderAuth(String),
    #[error("{0}")]
    ProviderCredits(String),
    #[error("{0}")]
    ProviderUnavailable(String),
    #[error("{0}")]
    MailAuth(String),
    #[error("{0}")]
    MailUnavailable(String),
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
            Self::ProviderAuth(_) => "ProviderAuth",
            Self::ProviderCredits(_) => "ProviderCredits",
            Self::ProviderUnavailable(_) => "ProviderUnavailable",
            Self::MailAuth(_) => "MailAuth",
            Self::MailUnavailable(_) => "MailUnavailable",
        }
    }
}

impl From<crate::mail::imap::MailError> for AppError {
    fn from(e: crate::mail::imap::MailError) -> Self {
        use crate::mail::imap::MailError;
        match e {
            MailError::Auth(_) => Self::MailAuth(e.to_string()),
            MailError::Connection(_) | MailError::Protocol(_) => Self::MailUnavailable(e.to_string()),
        }
    }
}

impl From<StoreError> for AppError {
    fn from(e: StoreError) -> Self {
        match &e {
            StoreError::Db(sqlx::Error::RowNotFound) | StoreError::NotFound => Self::NotFound("Not found.".into()),
            StoreError::Invalid(m) if m.starts_with("no ") => Self::NotFound(m.clone()),
            StoreError::Invalid(m) => Self::Io(m.clone()),
            _ => Self::Io(e.to_string()),
        }
    }
}

impl From<EngineError> for AppError {
    fn from(e: EngineError) -> Self {
        match e {
            EngineError::ContextOverflow => Self::ContextOverflow(e.to_string()),
            EngineError::Unauthorized(_) => Self::ProviderAuth(e.to_string()),
            EngineError::InsufficientCredits(_) => Self::ProviderCredits(e.to_string()),
            EngineError::RateLimited | EngineError::Network(_) | EngineError::ModelUnavailable(_) => {
                Self::ProviderUnavailable(e.to_string())
            }
            _ => Self::EngineUnavailable(e.to_string()),
        }
    }
}

impl From<crate::providers::ProviderError> for AppError {
    fn from(e: crate::providers::ProviderError) -> Self {
        match e {
            crate::providers::ProviderError::Missing(m) => Self::ModelMissing(m),
            crate::providers::ProviderError::Unauthorized | crate::providers::ProviderError::KeyRejected(_) => {
                Self::ProviderAuth(e.to_string())
            }
            crate::providers::ProviderError::Network(_) => Self::ProviderUnavailable(e.to_string()),
            _ => Self::EngineUnavailable(e.to_string()),
        }
    }
}

impl From<ResearchError> for AppError {
    fn from(e: ResearchError) -> Self {
        match e {
            ResearchError::Unavailable(m) => Self::ModelMissing(m),
            ResearchError::Search(p) => p.into(),
            other => Self::EngineUnavailable(other.to_string()),
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

pub async fn create_job(
    st: &AppState,
    title: &str,
    company: &str,
    raw_text: &str,
    url: &str,
    status_id: Option<i64>,
) -> Res<i64> {
    Ok(st.store.create_jd(title, company, raw_text, url, status_id).await?)
}

pub async fn update_job_tracking(st: &AppState, id: i64, status_id: Option<i64>, url: &str) -> Res<()> {
    if !st.store.update_jd_tracking(id, status_id, url).await? {
        return Err(AppError::NotFound(format!("Job description {id} not found.")));
    }
    Ok(())
}

pub async fn update_job_description(st: &AppState, id: i64, raw_text: &str) -> Res<()> {
    if !st.store.update_jd_text(id, raw_text).await? {
        return Err(AppError::NotFound(format!("Job description {id} not found.")));
    }
    Ok(())
}

/// Longest posting read from a URL. Pasted text has no limit.
const JOB_PAGE_CHARS: usize = 30_000;

/// Reads a posting's text from its URL, for when the user didn't paste the description.
pub async fn fetch_job_description(url: &str) -> Res<String> {
    web::fetch_text(url, FetchPolicy::PublicOnly, JOB_PAGE_CHARS).await.map_err(|e| AppError::Io(e.to_string()))
}

// ---- job statuses ----

pub async fn list_job_statuses(st: &AppState) -> Res<Vec<JobStatus>> {
    Ok(st.store.list_job_statuses().await?)
}

pub async fn create_job_status(st: &AppState, name: &str, color: StatusColor) -> Res<JobStatus> {
    let id = st.store.create_job_status(name, color).await?;
    status_by_id(st, id).await
}

pub async fn update_job_status(st: &AppState, id: i64, name: &str, color: StatusColor) -> Res<JobStatus> {
    if !st.store.update_job_status(id, name, color).await? {
        return Err(AppError::NotFound(format!("Status {id} not found.")));
    }
    status_by_id(st, id).await
}

pub async fn delete_job_status(st: &AppState, id: i64) -> Res<()> {
    Ok(st.store.delete_job_status(id).await?)
}

pub async fn reorder_job_statuses(st: &AppState, ids: Vec<i64>) -> Res<()> {
    Ok(st.store.reorder_job_statuses(&ids).await?)
}

async fn status_by_id(st: &AppState, id: i64) -> Res<JobStatus> {
    st.store
        .list_job_statuses()
        .await?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| AppError::NotFound(format!("Status {id} not found.")))
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
/// so `matcher::resume_match` scores it end to end. Company research runs
/// first and blocks scoring unless skipped (see `skip_research`).
fn spawn_run(st: &AppState, run_id: i64, model_id: &str) -> Res<()> {
    let engine = st.engine_for(model_id)?;
    let token = st
        .register_run(run_id)
        .ok_or_else(|| AppError::Io(format!("Match run {run_id} is already running.")))?;
    let (store, emit, runs) = (st.store.clone(), st.emitter(), st.runs());
    let engine_error = st.engine_error_slot();
    let (coordinator, env, skips) = (st.coordinator.clone(), st.research_env(), st.skips());
    let skip = st.register_skip(run_id);
    tokio::spawn(async move {
        let progress_emit = emit.clone();
        let on_progress = move |p| progress_emit(MATCH_EVENT, json!({ "run_id": run_id, "event": p }));
        let context = company_context(&store, &coordinator, &env, run_id, &token, skip, &on_progress).await;
        skips.lock().unwrap_or_else(|e| e.into_inner()).remove(&run_id);
        let outcome = match context {
            Ok(ctx) => {
                matcher::resume_match(&store, &*engine, run_id, MatchConfig::default(), ctx.as_deref(), token, on_progress)
                    .await
            }
            Err(e) => Err(e.into()),
        };
        match &outcome {
            Ok(_) => crate::state::set_engine_error(&engine_error, None),
            Err(MatchError::Engine(e)) if !matches!(e, EngineError::ContextOverflow) => {
                crate::state::set_engine_error(&engine_error, Some(e.to_string()))
            }
            _ => {}
        }
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
        emit(ENGINE_EVENT, json!({}));
    });
    Ok(())
}

/// Resolves the job's company, waits for its research when needed, records
/// which profile the run uses, and renders it for the System One state.
/// Research problems never fail the match: it scores without context.
async fn company_context(
    store: &Store,
    coordinator: &ResearchCoordinator,
    env: &ResearchEnv,
    run_id: i64,
    run_cancel: &CancellationToken,
    skip: CancellationToken,
    on_progress: &(impl Fn(matcher::Progress) + Send + Sync),
) -> Result<Option<String>, StoreError> {
    let jd_id = store.get_run(run_id).await?.jd_id;
    let Some(jd) = store.get_jd(jd_id).await? else { return Ok(None) };
    let (normalized, domain) = (company::normalize_name(&jd.company), company::domain_from_url(&jd.url));
    if normalized.is_empty() && domain.is_none() {
        store.set_run_company(run_id, None).await?;
        return Ok(None);
    }
    let found = match jd.company_id {
        Some(id) => store.get_company(id).await.ok(),
        None => None,
    };
    let c = match found {
        Some(c) => c,
        None => match store.find_company(domain.as_deref(), &normalized).await? {
            Some(c) => c,
            None => {
                let name = if jd.company.trim().is_empty() { domain.clone().unwrap_or_default() } else { jd.company.trim().to_string() };
                store.create_company(&name, &normalized, domain.as_deref()).await?
            }
        },
    };
    store.set_jd_company(jd_id, Some(c.id)).await?;
    if company::need(&c, chrono::Utc::now()) == company::Need::Research {
        on_progress(matcher::Progress::Researching { company_id: c.id });
        let input = ResearchInput {
            name: c.name.clone(),
            domain: c.domain.clone().or(domain),
            jd_snippet: jd.raw_text.chars().take(JD_SNIPPET_CHARS).collect(),
        };
        // Outcomes are persisted by the coordinator; here we only stop waiting.
        tokio::select! {
            _ = coordinator.ensure(store, c.clone(), input, env.research_fn()) => {}
            _ = skip.cancelled() => {}
            _ = run_cancel.cancelled() => {}
        }
    }
    // Research may have merged the company into another row; re-read through the job.
    let company_id = store.get_jd(jd_id).await?.and_then(|j| j.company_id);
    let current = match company_id {
        Some(id) => store.get_company(id).await.ok(),
        None => None,
    };
    // Any researched profile beats none, even a stale one whose refresh failed.
    let usable = current.filter(|c| c.researched_at.is_some());
    store.set_run_company(run_id, usable.as_ref().and_then(|c| c.researched_at.as_deref())).await?;
    Ok(usable.map(|c| company::render(&c.name, c.domain.as_deref(), &c.profile)))
}

/// Stops a run from waiting on company research. `mode` is `"cancel"` (also
/// stops the research) or `"background"` (research continues and is cached).
pub async fn skip_research(st: &AppState, run_id: i64, mode: &str) -> Res<()> {
    if mode == "cancel" {
        let jd_id = st.store.get_run(run_id).await?.jd_id;
        if let Some(company_id) = st.store.get_jd(jd_id).await?.and_then(|j| j.company_id) {
            st.coordinator.cancel(company_id);
        }
    }
    if st.skip(run_id) {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("Match run {run_id} is not waiting for company research.")))
    }
}

/// Scores the run's job again in a new run, with the company profile as it is now.
pub async fn rescore(st: &AppState, run_id: i64) -> Res<i64> {
    let jd_id = st.store.get_run(run_id).await?.jd_id;
    start_match(st, jd_id).await
}

pub async fn get_company(st: &AppState, id: i64) -> Res<Company> {
    st.store.get_company(id).await.map_err(|_| AppError::NotFound(format!("Company {id} not found.")))
}

pub async fn get_job_company(st: &AppState, jd_id: i64) -> Res<Option<Company>> {
    match get_job(st, jd_id).await?.company_id {
        Some(id) => Ok(st.store.get_company(id).await.ok()),
        None => Ok(None),
    }
}

/// A manual edit; a later refresh (or TTL expiry) overwrites it.
pub async fn edit_company(st: &AppState, id: i64, profile: CompanyProfile) -> Res<Company> {
    get_company(st, id).await?;
    Ok(st.store.edit_company(id, &profile).await?)
}

/// Researches the company now, ignoring the TTL and any backoff.
pub async fn refresh_company(st: &AppState, id: i64) -> Res<Company> {
    let c = get_company(st, id).await?;
    let input = ResearchInput { name: c.name.clone(), domain: c.domain.clone(), jd_snippet: String::new() };
    Ok(st.coordinator.start(&st.store, c, input, st.research_env().research_fn()).await?)
}

pub async fn list_runs(st: &AppState, jd_id: i64) -> Res<Vec<MatchRun>> {
    Ok(st.store.list_runs(jd_id).await?)
}

/// Every persisted score of a run, with raw probabilities (rubric breakdown,
/// role signals); the UI filters by target.
pub async fn get_scores(st: &AppState, run_id: i64) -> Res<Vec<ScoreRow>> {
    st.store.get_run(run_id).await?;
    Ok(st.store.scores_for_run(run_id).await?)
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
    if !(1..=2).contains(&budget) {
        return Err(AppError::Render(format!("page budget must be 1 or 2, got {budget}")));
    }
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

/// The webview supplies the path, so only absolute `.pdf` targets are allowed.
fn validate_export_path(path: &str) -> Res<()> {
    let p = std::path::Path::new(path);
    let pdf = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    if !p.is_absolute() || !pdf {
        return Err(AppError::Io(format!(
            "export path must be an absolute path ending in .pdf: {path}"
        )));
    }
    Ok(())
}

/// Compiles `selection` at the loosest style that needs no more pages than
/// the tightest one, writes the PDF to `path` and records the CV.
pub async fn export_cv(st: &AppState, run_id: i64, selection: Selection, path: String) -> Res<i64> {
    validate_export_path(&path)?;
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

pub async fn list_cvs(st: &AppState, jd_id: i64) -> Res<Vec<Cv>> {
    Ok(st.store.list_cvs(jd_id).await?)
}

// ---- settings ----

fn ui_key(key: &str) -> Res<()> {
    if key.starts_with(UI_SETTING_PREFIX) {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("Unknown setting {key}.")))
    }
}

pub async fn get_ui_setting(st: &AppState, key: &str) -> Res<Option<String>> {
    ui_key(key)?;
    Ok(st.store.get_setting(key).await?)
}

pub async fn set_ui_setting(st: &AppState, key: &str, value: &str) -> Res<()> {
    ui_key(key)?;
    Ok(st.store.set_setting(key, value).await?)
}

// ---- models ----

pub async fn engine_status(st: &AppState) -> Res<crate::state::EngineStatus> {
    st.engine_status().await
}

pub fn cancel_download(st: &AppState, id: &str) -> Res<()> {
    if st.cancel_download(id) {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("Model {id} is not downloading.")))
    }
}

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
    let token = st
        .register_download(spec.id)
        .ok_or_else(|| AppError::Io(format!("Model {id} is already downloading.")))?;
    let out = tokio::select! {
        r = download_attempts(st, spec) => r,
        _ = token.cancelled() => Err(AppError::Io(DOWNLOAD_PAUSED.into())),
    };
    st.finish_download(spec.id);
    out
}

async fn download_attempts(st: &AppState, spec: &'static ModelSpec) -> Res<()> {
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

/// A local model's kind; `NotFound` when the id is unknown.
fn local_kind(id: &str) -> Res<ModelKind> {
    REGISTRY
        .iter()
        .find(|s| s.id == id)
        .map(|s| s.kind)
        .ok_or_else(|| AppError::NotFound(format!("Unknown model {id}.")))
}

fn require_kind(id: &str, want: ModelKind) -> Res<()> {
    if local_kind(id)? != want {
        let role = if want == ModelKind::Chat { "Company research" } else { "System One" };
        return Err(AppError::NotFound(format!("Model {id} can't be used for {role}.")));
    }
    Ok(())
}

pub async fn set_research_model(st: &AppState, id: &str) -> Res<()> {
    let r = ModelRef::parse(id);
    if r.is_local() {
        require_kind(id, ModelKind::Chat)?;
        st.installed_path(id)?;
    } else {
        st.providers().check(&r)?;
    }
    st.store.set_setting(crate::state::RESEARCH_MODEL_KEY, id).await?;
    st.reset_research();
    Ok(())
}

pub async fn set_active_model(st: &AppState, id: &str) -> Res<()> {
    let r = ModelRef::parse(id);
    if r.is_local() {
        require_kind(id, ModelKind::Decision)?;
        st.installed_path(id)?;
    } else {
        st.providers().check(&r)?;
    }
    st.store.set_setting(crate::state::ACTIVE_MODEL_KEY, id).await?;
    st.reset_engine();
    (st.emitter())(ENGINE_EVENT, json!({}));
    Ok(())
}

// ---- providers ----

/// What the UI may know about a stored key.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct KeyStatus {
    pub has_key: bool,
    pub last4: Option<String>,
}

fn known_provider(provider: &str) -> Res<()> {
    if provider == OPENROUTER || provider == SUBQ || provider == EXA {
        Ok(())
    } else {
        Err(AppError::NotFound(format!("Unknown provider {provider}.")))
    }
}

fn key_status(key: Option<&str>) -> KeyStatus {
    KeyStatus {
        has_key: key.is_some(),
        last4: key.map(|k| k.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect()),
    }
}

/// Validates, stores and activates a provider key. Leading/trailing whitespace is dropped.
pub async fn set_provider_key(st: &AppState, provider: &str, key: &str) -> Res<KeyStatus> {
    known_provider(provider)?;
    let key = key.trim();
    if provider == EXA {
        ExaSearch::new(st.exa_base.clone(), key.into()).search("test", 1).await?;
    } else if provider == SUBQ {
        SubQProvider::new(st.subq_base.clone(), key.into()).validate_key().await?;
    } else {
        OpenRouterProvider::new(st.openrouter_base.clone(), key.into()).validate_key().await?;
    }
    st.store.set_secret(provider, key).await?;
    st.reload_providers().await?;
    st.reset_engine();
    (st.emitter())(ENGINE_EVENT, json!({}));
    Ok(key_status(Some(key)))
}

pub async fn delete_provider_key(st: &AppState, provider: &str) -> Res<()> {
    known_provider(provider)?;
    st.store.delete_secret(provider).await?;
    st.reload_providers().await?;
    st.reset_engine();
    (st.emitter())(ENGINE_EVENT, json!({}));
    Ok(())
}

pub async fn provider_key_status(st: &AppState, provider: &str) -> Res<KeyStatus> {
    known_provider(provider)?;
    Ok(key_status(st.store.get_secret(provider).await?.as_deref()))
}

/// `role` is `"decision"` (System One) or `"chat"` (company research).
pub async fn list_remote_models(st: &AppState, refresh: bool, role: &str) -> Res<Vec<ProviderModels>> {
    Ok(match role {
        "chat" => st.providers().remote_chat_models(refresh).await,
        _ => st.providers().remote_models(refresh).await,
    })
}

// ---- Gmail account and scan ----

/// The password never leaves Rust; the view only says whether one is stored.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MailAccountView {
    pub host: String,
    pub port: u16,
    pub address: String,
    pub has_password: bool,
    /// The mailbox the scan reads.
    pub folder: String,
}

fn mail_view(a: &MailAccount, folder: &str) -> MailAccountView {
    MailAccountView {
        host: a.host.clone(),
        port: a.port,
        address: a.address.clone(),
        has_password: !a.password.is_empty(),
        folder: folder.to_string(),
    }
}

/// A scan result plus the Gmail link, so the frontend never builds URLs.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessedEmailView {
    #[serde(flatten)]
    pub email: ProcessedEmail,
    pub gmail_url: String,
}

fn scan_view(email: ProcessedEmail) -> ProcessedEmailView {
    let gmail_url = crate::mail::imap::gmail_link(&email.message_id);
    ProcessedEmailView { email, gmail_url }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ScanState {
    pub running: bool,
}

/// `None` keeps the stored password; a new one is required when none is stored.
async fn resolve_password(st: &AppState, password: Option<&str>) -> Res<String> {
    match password.map(account::normalize_password).filter(|p| !p.is_empty()) {
        Some(p) => Ok(p),
        None => account::load(&st.store)
            .await?
            .map(|a| a.password)
            .ok_or_else(|| AppError::Io("Enter your Gmail app password.".into())),
    }
}

fn mail_account(host: &str, port: u16, address: &str, password: String) -> Res<MailAccount> {
    let (host, address) = (host.trim(), address.trim());
    account::validate(host, port, address).map_err(AppError::Io)?;
    Ok(MailAccount {
        host: host.to_string(),
        port,
        address: address.to_string(),
        password,
    })
}

pub async fn get_mail_account(st: &AppState) -> Res<Option<MailAccountView>> {
    let Some(account) = account::load(&st.store).await? else {
        return Ok(None);
    };
    let folder = account::folder(&st.store).await?;
    Ok(Some(mail_view(&account, &folder)))
}

/// `None` keeps the stored mailbox (or the default); an empty one is ignored.
pub async fn save_mail_account(
    st: &AppState,
    host: &str,
    port: u16,
    address: &str,
    password: Option<&str>,
    folder: Option<&str>,
) -> Res<MailAccountView> {
    let password = resolve_password(st, password).await?;
    let account = mail_account(host, port, address, password)?;
    account::save(&st.store, &account).await?;
    if let Some(folder) = folder.map(str::trim).filter(|f| !f.is_empty()) {
        account::set_folder(&st.store, folder).await?;
    }
    let folder = account::folder(&st.store).await?;
    Ok(mail_view(&account, &folder))
}


pub async fn delete_mail_account(st: &AppState) -> Res<()> {
    Ok(account::remove(&st.store).await?)
}

/// Logs in with the typed values and returns the mailboxes the account can
/// open. It does not open any folder, so it works before a mailbox is chosen.
pub async fn test_mail_account(
    st: &AppState,
    host: &str,
    port: u16,
    address: &str,
    password: Option<&str>,
) -> Res<Vec<String>> {
    let password = resolve_password(st, password).await?;
    let account = mail_account(host, port, address, password)?;
    Ok(ImapSource::new(account, account::DEFAULT_FOLDER.into())
        .list_mailboxes()
        .await?)
}

/// Claims the scan slot, creates the run, and scans in the background.
/// Progress and the final result arrive as `scan://` events.
pub async fn start_scan(st: &AppState) -> Res<i64> {
    let guard = st
        .begin_scan()
        .ok_or_else(|| AppError::Io("A scan is already running.".into()))?;
    let account = account::load(&st.store)
        .await?
        .ok_or_else(|| AppError::Io("Connect Gmail in Settings › Integrations first.".into()))?;
    let folder = account::folder(&st.store).await?;
    let model = st.active_model().await?;
    let engine = st.engine_for(&model)?;
    let chat = st.research_endpoint().await?;
    let run_id = scan::begin_run(&st.store).await?;

    let store = st.store.clone();
    let emit = st.emitter();
    let mail = ImapSource::new(account, folder);
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        let http = reqwest::Client::new();
        let deps = ScanDeps {
            store: &store,
            mail: &mail,
            engine: engine.as_ref(),
            chat: &chat,
            http: &http,
            emit: &emit,
            today: chrono::Local::now().date_naive(),
        };
        // The outcome reaches the UI through `scan://finished`.
        let _ = scan::run_scan(deps, run_id).await;
    });
    Ok(run_id)
}

pub fn get_scan_status(st: &AppState) -> ScanState {
    ScanState {
        running: st.scan_running(),
    }
}

pub async fn list_scan_results(st: &AppState, run_id: i64) -> Res<Vec<ProcessedEmailView>> {
    Ok(st.store.list_run_results(run_id).await?.into_iter().map(scan_view).collect())
}

pub async fn list_pending_reviews(st: &AppState) -> Res<Vec<ProcessedEmailView>> {
    Ok(st.store.list_pending_reviews().await?.into_iter().map(scan_view).collect())
}

pub async fn apply_suggestion(st: &AppState, message_id: &str) -> Res<ProcessedEmailView> {
    Ok(scan_view(st.store.apply_suggestion(message_id).await?))
}

pub async fn dismiss_suggestion(st: &AppState, message_id: &str) -> Res<()> {
    Ok(st.store.dismiss_suggestion(message_id).await?)
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
    pub async fn create_job(
        st: S<'_>,
        title: String,
        company: String,
        raw_text: String,
        url: String,
        status_id: Option<i64>,
    ) -> Res<i64> {
        super::create_job(&st, &title, &company, &raw_text, &url, status_id).await
    }
    #[tauri::command]
    pub async fn update_job_tracking(st: S<'_>, id: i64, status_id: Option<i64>, url: String) -> Res<()> {
        super::update_job_tracking(&st, id, status_id, &url).await
    }
    #[tauri::command]
    pub async fn update_job_description(st: S<'_>, id: i64, raw_text: String) -> Res<()> {
        super::update_job_description(&st, id, &raw_text).await
    }
    #[tauri::command]
    pub async fn fetch_job_description(url: String) -> Res<String> {
        super::fetch_job_description(&url).await
    }
    #[tauri::command]
    pub async fn list_job_statuses(st: S<'_>) -> Res<Vec<JobStatus>> {
        super::list_job_statuses(&st).await
    }
    #[tauri::command]
    pub async fn create_job_status(st: S<'_>, name: String, color: StatusColor) -> Res<JobStatus> {
        super::create_job_status(&st, &name, color).await
    }
    #[tauri::command]
    pub async fn update_job_status(st: S<'_>, id: i64, name: String, color: StatusColor) -> Res<JobStatus> {
        super::update_job_status(&st, id, &name, color).await
    }
    #[tauri::command]
    pub async fn delete_job_status(st: S<'_>, id: i64) -> Res<()> {
        super::delete_job_status(&st, id).await
    }
    #[tauri::command]
    pub async fn reorder_job_statuses(st: S<'_>, ids: Vec<i64>) -> Res<()> {
        super::reorder_job_statuses(&st, ids).await
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
    pub async fn list_runs(st: S<'_>, jd_id: i64) -> Res<Vec<MatchRun>> {
        super::list_runs(&st, jd_id).await
    }
    #[tauri::command]
    pub async fn get_scores(st: S<'_>, run_id: i64) -> Res<Vec<ScoreRow>> {
        super::get_scores(&st, run_id).await
    }
    #[tauri::command]
    pub async fn list_cvs(st: S<'_>, jd_id: i64) -> Res<Vec<Cv>> {
        super::list_cvs(&st, jd_id).await
    }
    #[tauri::command]
    pub async fn set_provider_key(st: S<'_>, provider: String, key: String) -> Res<KeyStatus> {
        super::set_provider_key(&st, &provider, &key).await
    }
    #[tauri::command]
    pub async fn delete_provider_key(st: S<'_>, provider: String) -> Res<()> {
        super::delete_provider_key(&st, &provider).await
    }
    #[tauri::command]
    pub async fn provider_key_status(st: S<'_>, provider: String) -> Res<KeyStatus> {
        super::provider_key_status(&st, &provider).await
    }
    #[tauri::command]
    pub async fn list_remote_models(st: S<'_>, refresh: bool, role: Option<String>) -> Res<Vec<ProviderModels>> {
        super::list_remote_models(&st, refresh, role.as_deref().unwrap_or("decision")).await
    }
    #[tauri::command]
    pub async fn get_ui_setting(st: S<'_>, key: String) -> Res<Option<String>> {
        super::get_ui_setting(&st, &key).await
    }
    #[tauri::command]
    pub async fn set_ui_setting(st: S<'_>, key: String, value: String) -> Res<()> {
        super::set_ui_setting(&st, &key, &value).await
    }
    #[tauri::command]
    pub async fn engine_status(st: S<'_>) -> Res<crate::state::EngineStatus> {
        super::engine_status(&st).await
    }
    #[tauri::command]
    pub fn cancel_download(st: S<'_>, id: String) -> Res<()> {
        super::cancel_download(&st, &id)
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
    #[tauri::command]
    pub async fn skip_research(st: S<'_>, run_id: i64, mode: String) -> Res<()> {
        super::skip_research(&st, run_id, &mode).await
    }
    #[tauri::command]
    pub async fn rescore(st: S<'_>, run_id: i64) -> Res<i64> {
        super::rescore(&st, run_id).await
    }
    #[tauri::command]
    pub async fn get_company(st: S<'_>, id: i64) -> Res<Company> {
        super::get_company(&st, id).await
    }
    #[tauri::command]
    pub async fn get_job_company(st: S<'_>, jd_id: i64) -> Res<Option<Company>> {
        super::get_job_company(&st, jd_id).await
    }
    #[tauri::command]
    pub async fn edit_company(st: S<'_>, id: i64, profile: CompanyProfile) -> Res<Company> {
        super::edit_company(&st, id, profile).await
    }
    #[tauri::command]
    pub async fn refresh_company(st: S<'_>, id: i64) -> Res<Company> {
        super::refresh_company(&st, id).await
    }
    #[tauri::command]
    pub async fn set_research_model(st: S<'_>, id: String) -> Res<()> {
        super::set_research_model(&st, &id).await
    }
    #[tauri::command]
    pub async fn get_research_model(st: S<'_>) -> Res<Option<String>> {
        st.research_model().await
    }
    #[tauri::command]
    pub async fn get_mail_account(st: S<'_>) -> Res<Option<MailAccountView>> {
        super::get_mail_account(&st).await
    }
    #[tauri::command]
    pub async fn save_mail_account(
        st: S<'_>,
        host: String,
        port: u16,
        address: String,
        password: Option<String>,
        folder: Option<String>,
    ) -> Res<MailAccountView> {
        super::save_mail_account(&st, &host, port, &address, password.as_deref(), folder.as_deref()).await
    }
    #[tauri::command]
    pub async fn delete_mail_account(st: S<'_>) -> Res<()> {
        super::delete_mail_account(&st).await
    }
    #[tauri::command]
    pub async fn test_mail_account(
        st: S<'_>,
        host: String,
        port: u16,
        address: String,
        password: Option<String>,
    ) -> Res<Vec<String>> {
        super::test_mail_account(&st, &host, port, &address, password.as_deref()).await
    }
    #[tauri::command]
    pub async fn start_scan(st: S<'_>) -> Res<i64> {
        super::start_scan(&st).await
    }
    #[tauri::command]
    pub async fn get_scan_status(st: S<'_>) -> Res<ScanState> {
        Ok(super::get_scan_status(&st))
    }
    #[tauri::command]
    pub async fn list_scan_results(st: S<'_>, run_id: i64) -> Res<Vec<ProcessedEmailView>> {
        super::list_scan_results(&st, run_id).await
    }
    #[tauri::command]
    pub async fn list_pending_reviews(st: S<'_>) -> Res<Vec<ProcessedEmailView>> {
        super::list_pending_reviews(&st).await
    }
    #[tauri::command]
    pub async fn apply_suggestion(st: S<'_>, message_id: String) -> Res<ProcessedEmailView> {
        super::apply_suggestion(&st, &message_id).await
    }
    #[tauri::command]
    pub async fn dismiss_suggestion(st: S<'_>, message_id: String) -> Res<()> {
        super::dismiss_suggestion(&st, &message_id).await
    }
}

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
