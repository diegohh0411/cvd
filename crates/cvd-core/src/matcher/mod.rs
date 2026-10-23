pub mod questions;

use std::collections::BTreeMap;

use tokio_util::sync::CancellationToken;

use crate::engine::{decide_checked, wire_request, Answer, DecisionEngine, EngineError, Question};
use crate::store::types::*;
use crate::store::{StoreError, TenantStore as Store};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchConfig {
    pub score_floor: f64,
    /// Estimated input tokens per request. Clef evaluates a request in one
    /// 16,384-token physical batch, so this leaves headroom for estimate error.
    pub token_budget: usize,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            score_floor: 1.5,
            token_budget: 12_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum Progress {
    /// Waiting for company research (see `company://research` for its steps).
    Researching {
        company_id: i64,
    },
    ReadingJd,
    /// `done`/`total` count questions across the run's batched requests.
    Scoring {
        done: usize,
        total: usize,
    },
    Done,
}

#[derive(Debug, thiserror::Error)]
pub enum MatchError {
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("job description {0} not found")]
    JdNotFound(i64),
    #[error("match run {run_id} cancelled")]
    Cancelled { run_id: i64 },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, ts_rs::TS)]
pub struct RankedEntry {
    pub entry: Entry,
    pub score: f64,
    pub bullets: Vec<(Bullet, f64)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, ts_rs::TS)]
pub struct Ranked {
    pub entries: Vec<RankedEntry>,
    pub skills: Vec<(Skill, f64)>,
    pub seniority: String,
    /// JD focus tags the model judged relevant, most probable first.
    pub tags: Vec<String>,
}

/// Minimum "yes" probability for a tag to be shown.
const TAG_THRESHOLD: f64 = 0.5;

/// Starts a new run for `jd_id` and scores it end to end.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub async fn run_match(
    store: &Store,
    engine: &dyn DecisionEngine,
    jd_id: i64,
    model_id: &str,
    cfg: MatchConfig,
    company: Option<&str>,
    cancel: CancellationToken,
    on_progress: impl Fn(Progress),
) -> Result<i64, MatchError> {
    if store.get_jd(jd_id).await?.is_none() {
        return Err(MatchError::JdNotFound(jd_id));
    }
    let run_id = store.create_run(jd_id, model_id).await?;
    finish(
        store,
        run_id,
        execute(
            store,
            engine,
            run_id,
            jd_id,
            cfg,
            company,
            &cancel,
            &on_progress,
        )
        .await,
    )
    .await
}

/// Scores the existing run `run_id` end to end.
pub async fn score_run(
    store: &Store,
    engine: &dyn DecisionEngine,
    run_id: i64,
    cfg: MatchConfig,
    company: Option<&str>,
    cancel: CancellationToken,
    on_progress: impl Fn(Progress),
) -> Result<i64, MatchError> {
    let run = store.get_run(run_id).await?;
    store.set_run_status(run_id, RunStatus::Running).await?;
    finish(
        store,
        run_id,
        execute(
            store,
            engine,
            run_id,
            run.jd_id,
            cfg,
            company,
            &cancel,
            &on_progress,
        )
        .await,
    )
    .await
}

/// Records the terminal status for the outcome of `execute`.
async fn finish(
    store: &Store,
    run_id: i64,
    outcome: Result<(), MatchError>,
) -> Result<i64, MatchError> {
    let status = match &outcome {
        Ok(()) => RunStatus::Done,
        Err(MatchError::Cancelled { .. }) => RunStatus::Cancelled,
        Err(_) => RunStatus::Failed,
    };
    let saved = store.set_run_status(run_id, status).await;
    // A failed status save must not mask the original engine/cancel error.
    match outcome {
        Ok(()) => saved.map(|()| run_id).map_err(MatchError::from),
        Err(e) => Err(e),
    }
}

fn check_cancel(cancel: &CancellationToken, run_id: i64) -> Result<(), MatchError> {
    if cancel.is_cancelled() {
        Err(MatchError::Cancelled { run_id })
    } else {
        Ok(())
    }
}

fn yes_no(p: f64) -> BTreeMap<String, f64> {
    BTreeMap::from([("yes".to_string(), p), ("no".to_string(), 1.0 - p)])
}

/// Maps one engine answer to a score row.
fn to_row(run_id: i64, key: &str, answer: &Answer, target: (TargetType, i64)) -> ScoreRow {
    let (probabilities, score) = match answer {
        Answer::Noul { p } => (yes_no(*p), *p),
        Answer::Choice {
            choice,
            probabilities,
        } => (
            probabilities.clone(),
            probabilities.get(choice).copied().unwrap_or(0.0),
        ),
        Answer::Score {
            score,
            probabilities,
        } => (probabilities.clone(), *score),
    };
    ScoreRow {
        run_id,
        target_type: target.0,
        target_id: target.1,
        question: key.to_string(),
        probabilities,
        score,
    }
}

/// One question in a batch: the request id (unique within a run), the
/// question name stored on the score row, and the row's target.
struct Pending {
    id: String,
    stored: String,
    question: Question,
    target: (TargetType, i64),
}

impl Pending {
    fn new(id: String, stored: &str, question: Question, target: (TargetType, i64)) -> Self {
        Self {
            id,
            stored: stored.to_string(),
            question,
            target,
        }
    }
}

/// Rough input-token estimate. The model prompt wraps every question in a
/// template (~80 tokens, measured in the spike) on top of its JSON text.
fn estimate_tokens(text_len: usize) -> usize {
    text_len / 3
}

fn question_tokens(p: &Pending) -> usize {
    let one = BTreeMap::from([(p.id.clone(), p.question.clone())]);
    80 + estimate_tokens(wire_request("", &one)["questions"].to_string().len())
}

/// Splits questions into requests under both the question cap and the token
/// budget. With a cap of `n`, `q` questions take `ceil(q / n)` requests unless
/// the token budget splits them further.
fn batches(questions: Vec<Pending>, state: &str, budget: usize, cap: usize) -> Vec<Vec<Pending>> {
    let base = 50 + estimate_tokens(state.len());
    let mut out: Vec<Vec<Pending>> = Vec::new();
    let mut cur: Vec<Pending> = Vec::new();
    let mut used = base;
    for p in questions {
        let t = question_tokens(&p);
        if !cur.is_empty() && (cur.len() == cap || used + t > budget) {
            out.push(std::mem::take(&mut cur));
            used = base;
        }
        used += t;
        cur.push(p);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Calls the engine and persists every answer immediately. A request the
/// model reports as too large is split in half and retried.
async fn ask(
    store: &Store,
    engine: &dyn DecisionEngine,
    run_id: i64,
    state: &str,
    mut batch: Vec<Pending>,
) -> Result<(), MatchError> {
    let qs: BTreeMap<String, Question> = batch
        .iter()
        .map(|p| (p.id.clone(), p.question.clone()))
        .collect();
    let answers = match decide_checked(engine, state, &qs).await {
        Err(EngineError::ContextOverflow) if batch.len() > 1 => {
            let rest = batch.split_off(batch.len() / 2);
            Box::pin(ask(store, engine, run_id, state, batch)).await?;
            return Box::pin(ask(store, engine, run_id, state, rest)).await;
        }
        other => other?,
    };
    let mut rows = Vec::with_capacity(batch.len());
    for p in &batch {
        let a = answers
            .get(&p.id)
            .ok_or_else(|| EngineError::BadResponse(format!("missing answer for {}", p.id)))?;
        rows.push(to_row(run_id, &p.stored, a, p.target));
    }
    store.insert_scores(&rows).await?;
    Ok(())
}

/// Asks every question about the JD in as few requests as fit: JD meta,
/// skills, entry relevance and seniority fit, and every bullet. The JD is the
/// shared state; each entry or bullet question carries its own data.
#[allow(clippy::too_many_arguments)]
async fn execute(
    store: &Store,
    engine: &dyn DecisionEngine,
    run_id: i64,
    jd_id: i64,
    cfg: MatchConfig,
    company: Option<&str>,
    cancel: &CancellationToken,
    on_progress: &impl Fn(Progress),
) -> Result<(), MatchError> {
    let jd = store
        .get_jd(jd_id)
        .await?
        .ok_or(MatchError::JdNotFound(jd_id))?;
    on_progress(Progress::ReadingJd);

    let mut all = vec![
        Pending::new(
            questions::SENIORITY_KEY.into(),
            questions::SENIORITY_KEY,
            questions::seniority_question(),
            (TargetType::Meta, 0),
        ),
        Pending::new(
            questions::ROLE_FAMILY_KEY.into(),
            questions::ROLE_FAMILY_KEY,
            questions::role_family_question(),
            (TargetType::Meta, 0),
        ),
    ];
    for tag in questions::JD_TAGS {
        let key = questions::tag_key(tag);
        all.push(Pending::new(
            key.clone(),
            &key,
            questions::tag_question(tag),
            (TargetType::Meta, 0),
        ));
    }
    for s in store.list_skills().await? {
        let key = questions::skill_key(s.id);
        all.push(Pending::new(
            key.clone(),
            &key,
            questions::skill_question(&s.name),
            (TargetType::Skill, s.id),
        ));
    }
    for e in &store.list_entries(false).await? {
        let target = (TargetType::Entry, e.id);
        all.push(Pending::new(
            questions::relevance_key(e.id),
            questions::RELEVANCE_KEY,
            questions::entry_question(e),
            target,
        ));
        all.push(Pending::new(
            questions::seniority_fit_key(e.id),
            questions::SENIORITY_FIT_KEY,
            questions::seniority_fit_question(e),
            target,
        ));
        for b in &e.bullets {
            let key = questions::bullet_key(b.id);
            all.push(Pending::new(
                key.clone(),
                &key,
                questions::bullet_question(e, b),
                (TargetType::Bullet, b.id),
            ));
        }
    }
    let total = all.len();
    let mut done = 0;
    on_progress(Progress::Scoring { done, total });

    let state = questions::jd_state(&jd.raw_text, company);
    let cap = engine.max_questions();
    for batch in batches(all, &state, cfg.token_budget, cap) {
        check_cancel(cancel, run_id)?;
        let n = batch.len();
        ask(store, engine, run_id, &state, batch).await?;
        done += n;
        on_progress(Progress::Scoring { done, total });
    }
    on_progress(Progress::Done);
    Ok(())
}

/// Pure ranking of a run's scores: entries by relevance (desc), bullets by
/// score (desc; unscored bullets follow in position order with 0.0), skills
/// by probability (desc), and the most probable seniority.
pub fn rank(store_scores: &[ScoreRow], entries: &[Entry], skills: &[Skill]) -> Ranked {
    let score_of = |t: TargetType, id: i64, q: Option<&str>| {
        store_scores
            .iter()
            .find(|r| r.target_type == t && r.target_id == id && q.is_none_or(|q| r.question == q))
            .map(|r| r.score)
    };
    let desc = |a: &f64, b: &f64| b.total_cmp(a);

    let mut ranked: Vec<RankedEntry> = entries
        .iter()
        .map(|e| {
            let mut scored: Vec<(Bullet, f64)> = Vec::new();
            let mut unscored: Vec<(Bullet, f64)> = Vec::new();
            for b in &e.bullets {
                match score_of(TargetType::Bullet, b.id, None) {
                    Some(s) => scored.push((b.clone(), s)),
                    None => unscored.push((b.clone(), 0.0)),
                }
            }
            scored.sort_by(|a, b| desc(&a.1, &b.1));
            scored.extend(unscored);
            RankedEntry {
                entry: e.clone(),
                score: score_of(TargetType::Entry, e.id, Some(questions::RELEVANCE_KEY))
                    .unwrap_or(0.0),
                bullets: scored,
            }
        })
        .collect();
    ranked.sort_by(|a, b| desc(&a.score, &b.score));

    let mut skills: Vec<(Skill, f64)> = skills
        .iter()
        .filter_map(|s| score_of(TargetType::Skill, s.id, None).map(|p| (s.clone(), p)))
        .collect();
    skills.sort_by(|a, b| desc(&a.1, &b.1));

    let seniority = store_scores
        .iter()
        .find(|r| r.target_type == TargetType::Meta && r.question == questions::SENIORITY_KEY)
        .and_then(|r| r.probabilities.iter().max_by(|a, b| a.1.total_cmp(b.1)))
        .map(|(k, _)| k.clone())
        .unwrap_or_default();

    let mut tags: Vec<(String, f64)> = store_scores
        .iter()
        .filter(|r| r.target_type == TargetType::Meta && r.score >= TAG_THRESHOLD)
        .filter_map(|r| {
            Some((
                r.question.strip_prefix(questions::TAG_PREFIX)?.to_string(),
                r.score,
            ))
        })
        .collect();
    tags.sort_by(|a, b| desc(&a.1, &b.1));

    Ranked {
        entries: ranked,
        skills,
        seniority,
        tags: tags.into_iter().map(|(t, _)| t).collect(),
    }
}

#[cfg(test)]
mod tests;
