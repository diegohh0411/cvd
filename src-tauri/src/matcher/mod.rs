pub mod questions;

use std::collections::BTreeMap;

use tokio_util::sync::CancellationToken;

use crate::engine::{decide_checked, Answer, DecisionEngine, EngineError, Question, MAX_QUESTIONS};
use crate::store::types::*;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MatchConfig {
    pub top_k: usize,
    pub score_floor: f64,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            top_k: 8,
            score_floor: 1.5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    ReadingJd,
    ScoringEntries { done: usize, total: usize },
    PickingBullets { done: usize, total: usize },
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

#[derive(Debug, Clone, PartialEq)]
pub struct RankedEntry {
    pub entry: Entry,
    pub score: f64,
    pub bullets: Vec<(Bullet, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    pub entries: Vec<RankedEntry>,
    pub skills: Vec<(Skill, f64)>,
    pub seniority: String,
}

/// Starts a new run for `jd_id` and scores it end to end.
pub async fn run_match(
    store: &Store,
    engine: &dyn DecisionEngine,
    jd_id: i64,
    model_id: &str,
    cfg: MatchConfig,
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
        execute(store, engine, run_id, jd_id, cfg, &cancel, &on_progress).await,
    )
    .await
}

/// Continues a cancelled or failed run, scoring only targets without scores.
pub async fn resume_match(
    store: &Store,
    engine: &dyn DecisionEngine,
    run_id: i64,
    cfg: MatchConfig,
    cancel: CancellationToken,
    on_progress: impl Fn(Progress),
) -> Result<i64, MatchError> {
    let run = store.get_run(run_id).await?;
    store.set_run_status(run_id, RunStatus::Running).await?;
    finish(
        store,
        run_id,
        execute(store, engine, run_id, run.jd_id, cfg, &cancel, &on_progress).await,
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
    store.set_run_status(run_id, status).await?;
    outcome.map(|()| run_id)
}

fn check_cancel(cancel: &CancellationToken, run_id: i64) -> Result<(), MatchError> {
    if cancel.is_cancelled() {
        Err(MatchError::Cancelled { run_id })
    } else {
        Ok(())
    }
}

fn has(rows: &[ScoreRow], t: TargetType, id: i64, question: Option<&str>) -> bool {
    rows.iter().any(|r| {
        r.target_type == t && r.target_id == id && question.is_none_or(|q| r.question == q)
    })
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

/// Calls the engine and persists every answer immediately.
async fn ask(
    store: &Store,
    engine: &dyn DecisionEngine,
    run_id: i64,
    state: &str,
    questions: Vec<(String, Question, (TargetType, i64))>,
) -> Result<(), MatchError> {
    let qs: BTreeMap<String, Question> = questions
        .iter()
        .map(|(k, q, _)| (k.clone(), q.clone()))
        .collect();
    let answers = decide_checked(engine, state, &qs).await?;
    let mut rows = Vec::with_capacity(questions.len());
    for (key, _, target) in &questions {
        let a = answers
            .get(key)
            .ok_or_else(|| EngineError::BadResponse(format!("missing answer for {key}")))?;
        rows.push(to_row(run_id, key, a, *target));
    }
    store.insert_scores(&rows).await?;
    Ok(())
}

async fn execute(
    store: &Store,
    engine: &dyn DecisionEngine,
    run_id: i64,
    jd_id: i64,
    cfg: MatchConfig,
    cancel: &CancellationToken,
    on_progress: &impl Fn(Progress),
) -> Result<(), MatchError> {
    let jd = store
        .get_jd(jd_id)
        .await?
        .ok_or(MatchError::JdNotFound(jd_id))?;
    let existing = store.scores_for_run(run_id).await?;

    // 1. JD: seniority + role family + one noul per skill, batched.
    on_progress(Progress::ReadingJd);
    let mut jd_qs = Vec::new();
    for (key, q) in [
        (questions::SENIORITY_KEY, questions::seniority_question()),
        (
            questions::ROLE_FAMILY_KEY,
            questions::role_family_question(),
        ),
    ] {
        if !has(&existing, TargetType::Meta, 0, Some(key)) {
            jd_qs.push((key.to_string(), q, (TargetType::Meta, 0)));
        }
    }
    for s in store.list_skills().await? {
        if !has(&existing, TargetType::Skill, s.id, None) {
            jd_qs.push((
                questions::skill_key(s.id),
                questions::skill_question(&s.name),
                (TargetType::Skill, s.id),
            ));
        }
    }
    let jd_state = questions::jd_state(&jd.raw_text);
    while !jd_qs.is_empty() {
        let rest = jd_qs.split_off(jd_qs.len().min(MAX_QUESTIONS));
        check_cancel(cancel, run_id)?;
        ask(
            store,
            engine,
            run_id,
            &jd_state,
            std::mem::replace(&mut jd_qs, rest),
        )
        .await?;
    }

    // 2. Entries: relevance score + seniority-fit noul, one call each.
    let entries = store.list_entries(false).await?;
    let total = entries.len();
    let mut done = entries
        .iter()
        .filter(|e| {
            has(
                &existing,
                TargetType::Entry,
                e.id,
                Some(questions::RELEVANCE_KEY),
            )
        })
        .count();
    on_progress(Progress::ScoringEntries { done, total });
    for e in &entries {
        if has(
            &existing,
            TargetType::Entry,
            e.id,
            Some(questions::RELEVANCE_KEY),
        ) {
            continue;
        }
        check_cancel(cancel, run_id)?;
        let target = (TargetType::Entry, e.id);
        let qs = vec![
            (
                questions::RELEVANCE_KEY.to_string(),
                questions::entry_question(),
                target,
            ),
            (
                questions::SENIORITY_FIT_KEY.to_string(),
                questions::seniority_fit_question(),
                target,
            ),
        ];
        ask(
            store,
            engine,
            run_id,
            &questions::entry_state(&jd.raw_text, e),
            qs,
        )
        .await?;
        done += 1;
        on_progress(Progress::ScoringEntries { done, total });
    }

    // 3. Bullets for the top-K entries; the score floor belongs to page-fit.
    let scores = store.scores_for_run(run_id).await?;
    let ranked = rank(&scores, &entries, &[]);
    let picked: Vec<&RankedEntry> = ranked
        .entries
        .iter()
        .take(cfg.top_k)
        .filter(|r| !r.entry.bullets.is_empty())
        .collect();
    let total = picked.len();
    on_progress(Progress::PickingBullets { done: 0, total });
    for (i, r) in picked.iter().enumerate() {
        let qs: Vec<_> = r
            .entry
            .bullets
            .iter()
            .filter(|b| !has(&scores, TargetType::Bullet, b.id, None))
            .map(|b| {
                (
                    questions::bullet_key(b.id),
                    questions::bullet_question(),
                    (TargetType::Bullet, b.id),
                )
            })
            .collect();
        for chunk in qs.chunks(MAX_QUESTIONS) {
            check_cancel(cancel, run_id)?;
            ask(
                store,
                engine,
                run_id,
                &questions::entry_state(&jd.raw_text, &r.entry),
                chunk.to_vec(),
            )
            .await?;
        }
        on_progress(Progress::PickingBullets { done: i + 1, total });
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

    Ranked {
        entries: ranked,
        skills,
        seniority,
    }
}

#[cfg(test)]
mod tests;
