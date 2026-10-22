//! One research run per company at a time, a global cap on concurrent runs,
//! and persistence of the outcome. Runs are spawned, so a waiter that stops
//! waiting (e.g. "continue in background") never stops the research.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::future::{BoxFuture, FutureExt, Shared};
use serde_json::json;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

use super::agent::{ResearchError, ResearchInput, ResearchOutput, ResearchStep};
use super::{backoff, format_ts, is_job_board, need, normalize_name, Need};
use crate::state::Emitter;
use crate::store::types::Company;
use crate::store::Store;

pub const RESEARCH_EVENT: &str = "company://research";

/// Runs the agent: `(input, cancel, on_step) -> (output, model id)`.
pub type ResearchFn = Arc<
    dyn Fn(
            ResearchInput,
            CancellationToken,
            Box<dyn Fn(ResearchStep) + Send + Sync>,
        ) -> BoxFuture<'static, Result<(ResearchOutput, String), ResearchError>>
        + Send
        + Sync,
>;

type Outcome = Shared<BoxFuture<'static, Result<Company, ResearchError>>>;

pub struct ResearchCoordinator {
    slots: Arc<Semaphore>,
    /// Serializes find/merge/save so concurrent runs for overlapping companies can't race.
    persist_lock: Arc<tokio::sync::Mutex<()>>,
    inflight: Arc<Mutex<HashMap<i64, (Outcome, CancellationToken)>>>,
    emit: Emitter,
}

fn emit_stage(emit: &Emitter, company_id: i64, event: serde_json::Value) {
    emit(
        RESEARCH_EVENT,
        json!({ "company_id": company_id, "event": event }),
    );
}

impl ResearchCoordinator {
    pub fn new(max_concurrent: usize, emit: Emitter) -> Self {
        Self {
            slots: Arc::new(Semaphore::new(max_concurrent.max(1))),
            persist_lock: Arc::default(),
            inflight: Arc::default(),
            emit,
        }
    }

    /// The company with a fresh profile: returned as is when fresh or backing
    /// off, otherwise after joining (or starting) its research run.
    pub async fn ensure(
        &self,
        store: &Store,
        company: Company,
        input: ResearchInput,
        run: ResearchFn,
    ) -> Result<Company, ResearchError> {
        if need(&company, chrono::Utc::now()) != Need::Research {
            return Ok(company);
        }
        self.start(store, company, input, run).await
    }

    /// Researches now regardless of TTL or backoff (manual refresh), joining a running run.
    pub async fn start(
        &self,
        store: &Store,
        company: Company,
        input: ResearchInput,
        run: ResearchFn,
    ) -> Result<Company, ResearchError> {
        let outcome = {
            let mut map = self.inflight.lock().unwrap_or_else(|e| e.into_inner());
            match map.get(&company.id) {
                Some((o, _)) => o.clone(),
                None => {
                    let token = CancellationToken::new();
                    let task = tokio::spawn(execute(
                        store.clone(),
                        company.clone(),
                        input,
                        run,
                        token.clone(),
                        self.slots.clone(),
                        self.persist_lock.clone(),
                        self.inflight.clone(),
                        self.emit.clone(),
                    ));
                    let o: Outcome = async move {
                        task.await.unwrap_or_else(|e| {
                            Err(ResearchError::Model(format!("research task failed: {e}")))
                        })
                    }
                    .boxed()
                    .shared();
                    map.insert(company.id, (o.clone(), token));
                    o
                }
            }
        };
        outcome.await
    }

    /// Cancels the running research for `company_id`; false when none runs.
    pub fn cancel(&self, company_id: i64) -> bool {
        match self
            .inflight
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&company_id)
        {
            Some((_, token)) => {
                token.cancel();
                true
            }
            None => false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute(
    store: Store,
    company: Company,
    input: ResearchInput,
    run: ResearchFn,
    cancel: CancellationToken,
    slots: Arc<Semaphore>,
    persist_lock: Arc<tokio::sync::Mutex<()>>,
    inflight: Arc<Mutex<HashMap<i64, (Outcome, CancellationToken)>>>,
    emit: Emitter,
) -> Result<Company, ResearchError> {
    let id = company.id;
    emit_stage(&emit, id, json!({ "stage": "queued" }));
    let result = async {
        let _permit = tokio::select! {
            _ = cancel.cancelled() => return Err(ResearchError::Cancelled),
            p = slots.acquire_owned() => p.map_err(|e| ResearchError::Model(e.to_string()))?,
        };
        emit_stage(&emit, id, json!({ "stage": "started" }));
        let step_emit = emit.clone();
        let on_step = Box::new(move |s: ResearchStep| {
            emit_stage(&step_emit, id, json!({ "stage": "step", "step": s }))
        });
        let (out, model_id) = run(input, cancel.clone(), on_step).await?;
        let _guard = persist_lock.lock().await;
        persist(&store, &company, &out, &model_id).await
    }
    .await;
    let result = match result {
        Err(ResearchError::Cancelled) => {
            emit_stage(&emit, id, json!({ "stage": "cancelled" }));
            Err(ResearchError::Cancelled)
        }
        Err(e @ (ResearchError::Unavailable(_) | ResearchError::Persist(_))) => {
            emit_stage(
                &emit,
                id,
                json!({ "stage": "failed", "error": e.to_string() }),
            );
            Err(e)
        }
        Err(e) => {
            let retry = chrono::Utc::now()
                + chrono::Duration::from_std(backoff(company.fail_count + 1))
                    .expect("backoff fits");
            // Best effort: the error the user sees is the research error.
            let _ = store
                .mark_research_failed(id, &e.to_string(), &format_ts(retry))
                .await;
            emit_stage(
                &emit,
                id,
                json!({ "stage": "failed", "error": e.to_string() }),
            );
            Err(e)
        }
        Ok(c) => {
            emit_stage(&emit, id, json!({ "stage": "ready", "company": c }));
            Ok(c)
        }
    };
    inflight
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&id);
    result
}

/// Saves the profile. The row is folded into another only when the agent's
/// canonical name matches it (a shared domain alone never merges: parents and
/// subsidiaries share domains). A domain owned by another row, or a job board,
/// is not stored. Callers hold the persist lock.
async fn persist(
    store: &Store,
    company: &Company,
    out: &ResearchOutput,
    model_id: &str,
) -> Result<Company, ResearchError> {
    let db = |e: crate::store::StoreError| ResearchError::Persist(e.to_string());
    // Another run may have merged this row away while we researched; follow its alias.
    let mut target = match store.get_company(company.id).await {
        Ok(c) => c.id,
        Err(_) => store
            .find_company(None, &company.normalized_name)
            .await
            .map_err(db)?
            .map(|c| c.id)
            .ok_or_else(|| ResearchError::Persist("the company was removed".into()))?,
    };
    let canonical = normalize_name(&out.canonical_name);
    if !canonical.is_empty() {
        if let Some(other) = store.find_company(None, &canonical).await.map_err(db)? {
            if other.id != target {
                let (keep, drop) = (other.id.min(target), other.id.max(target));
                store.merge_company(drop, keep).await.map_err(db)?;
                target = keep;
            }
        }
        store.add_alias(&canonical, target).await.map_err(db)?;
    }
    let domain = match out.domain.as_deref().filter(|d| !is_job_board(d)) {
        Some(d) => match store.find_company(Some(d), "").await.map_err(db)? {
            Some(owner) if owner.id != target => None,
            _ => Some(d),
        },
        None => None,
    };
    store
        .save_research(target, &out.canonical_name, domain, &out.profile, model_id)
        .await
        .map_err(db)
}
