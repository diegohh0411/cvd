//! One scan cycle shared by the watcher and "Check now": an optional re-check,
//! then the incremental scan, all inside the single scan slot.

use std::sync::Arc;

use super::account::{self, MailAccount};
use super::imap::ImapSource;
use super::recheck;
use super::scan::{self, ScanDeps, ScanSummary};
use crate::commands::AppError;
use crate::engine::DecisionEngine;
use crate::providers::ChatEndpoint;
use crate::state::{AppState, Emitter, ScanGuard};
use crate::store::{ScanSource, Store};

#[derive(Debug)]
pub enum CycleError {
    /// Another scan holds the slot.
    Busy,
    /// No Gmail account is saved.
    NoAccount,
    Failed(AppError),
}

impl From<AppError> for CycleError {
    fn from(e: AppError) -> Self {
        CycleError::Failed(e)
    }
}

impl From<crate::store::StoreError> for CycleError {
    fn from(e: crate::store::StoreError) -> Self {
        CycleError::Failed(e.into())
    }
}

/// Everything a cycle needs, owned so it can move into a spawned task.
/// Holding it holds the scan slot.
pub struct Prepared {
    _guard: ScanGuard,
    pub store: Store,
    account: MailAccount,
    folder: String,
    engine: Arc<dyn DecisionEngine>,
    chat: ChatEndpoint,
    emit: Emitter,
}

/// Claims the slot and gathers the cycle's dependencies. Nothing is written
/// to the database, so a refused cycle leaves no run behind.
pub async fn prepare(st: &AppState) -> Result<Prepared, CycleError> {
    let guard = st.begin_scan().ok_or(CycleError::Busy)?;
    let account = account::load(&st.store)
        .await?
        .ok_or(CycleError::NoAccount)?;
    let folder = account::folder(&st.store).await?;
    scan::ensure_ready(&st.store).await?;
    let model = st.active_model().await?;
    let engine = st.engine_for(&model)?;
    let chat = st.research_endpoint().await?;
    Ok(Prepared {
        _guard: guard,
        store: st.store.clone(),
        account,
        folder,
        engine,
        chat,
        emit: st.emitter(),
    })
}

/// Runs a re-check if a job was added, then the scan for `run_id` (created by
/// the caller with `scan::begin_run(&p.store, source)`). The slot frees when
/// this returns.
pub async fn run_cycle(
    p: Prepared,
    source: ScanSource,
    run_id: i64,
) -> Result<ScanSummary, AppError> {
    let mail = ImapSource::new(p.account.clone(), p.folder.clone());
    let http = reqwest::Client::new();
    let deps = || ScanDeps {
        store: &p.store,
        mail: &mail,
        engine: p.engine.as_ref(),
        chat: &p.chat,
        http: &http,
        emit: &p.emit,
        today: chrono::Local::now().date_naive(),
    };
    if recheck::needs_recheck(&p.store).await? {
        let recheck_run = p.store.create_scan_run(ScanSource::Recheck).await?;
        if let Err(e) = recheck::run_recheck(deps(), recheck_run).await {
            // The main run was already created; close it with the same error.
            let _ = scan::run_scan_failed(deps(), run_id, source, e.clone()).await;
            return Err(e);
        }
    }
    scan::run_scan(deps(), run_id, source).await
}
