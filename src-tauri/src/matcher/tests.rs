use std::collections::BTreeMap;
use std::sync::Mutex;

#[tokio::test]
async fn low_scoring_top_k_entry_still_gets_bullets() {
    // Both entries score 0.5 (below the 1.5 floor) but are within top_k.
    let (s, jd) = setup(2, 1, 0).await;
    let e = Recorder::new(|_| 0.5);
    let run = run_match(&s, &e, jd, "m", cfg(), CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(e.count(is_bullet_call), 2);
    let rows = s.scores_for_run(run).await.unwrap();
    let bullets = rows.iter().filter(|r| r.target_type == TargetType::Bullet).count();
    assert_eq!(bullets, 2);
}

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use super::questions::*;
use super::*;
use crate::engine::{Answer, EngineError, FakeDecisionEngine, Question};

/// Wraps the fake and records the question keys of every `decide` call.
/// (`FakeDecisionEngine::calls()` counts questions, not requests.)
struct Recorder {
    inner: FakeDecisionEngine,
    requests: Mutex<Vec<Vec<String>>>,
}

impl Recorder {
    fn new(entry_score: impl Fn(&str) -> f64 + Send + Sync + 'static) -> Self {
        let inner = FakeDecisionEngine::new(move |state, _name, q| match q {
            Question::Noul { .. } => Answer::Noul { p: 0.7 },
            Question::Choice { options, .. } => Answer::Choice {
                choice: options[0].clone(),
                probabilities: options
                    .iter()
                    .enumerate()
                    .map(|(i, o)| (o.clone(), if i == 0 { 0.9 } else { 0.1 / 6.0 }))
                    .collect(),
            },
            Question::Score { .. } => {
                let s = entry_score(state);
                Answer::Score {
                    score: s,
                    probabilities: BTreeMap::from([("3".into(), 1.0)]),
                }
            }
        });
        Self {
            inner,
            requests: Mutex::new(Vec::new()),
        }
    }

    fn requests(&self) -> Vec<Vec<String>> {
        self.requests.lock().unwrap().clone()
    }

    fn count(&self, pred: impl Fn(&[String]) -> bool) -> usize {
        self.requests().iter().filter(|r| pred(r)).count()
    }
}

#[async_trait]
impl DecisionEngine for Recorder {
    async fn decide(
        &self,
        state: &str,
        questions: &BTreeMap<String, Question>,
    ) -> Result<BTreeMap<String, Answer>, EngineError> {
        self.requests
            .lock()
            .unwrap()
            .push(questions.keys().cloned().collect());
        self.inner.decide(state, questions).await
    }
}

fn is_skill_call(r: &[String]) -> bool {
    r.iter().any(|k| k.starts_with("skill:"))
}
fn is_bullet_call(r: &[String]) -> bool {
    r.iter().any(|k| k.starts_with("bullet:"))
}
fn is_entry_call(r: &[String]) -> bool {
    r.iter().any(|k| k == RELEVANCE_KEY)
}

fn entry_input(title: &str, bullets: usize, skills: Vec<String>) -> EntryInput {
    EntryInput {
        id: None,
        kind: EntryKind::Experience,
        title: title.into(),
        org: "Acme".into(),
        location: String::new(),
        start_date: "2022-01".into(),
        end_date: None,
        description: "did things".into(),
        sort_hint: 0,
        archived: false,
        bullets: (0..bullets)
            .map(|i| format!("{title} bullet {i}"))
            .collect(),
        skills,
    }
}

async fn setup(entries: usize, bullets: usize, skills: usize) -> (Store, i64) {
    let s = Store::open("sqlite::memory:").await.unwrap();
    for i in 0..entries {
        let sk = if i == 0 {
            (0..skills).map(|k| format!("Skill{k}")).collect()
        } else {
            vec![]
        };
        s.upsert_entry(&entry_input(&format!("Entry{i}"), bullets, sk))
            .await
            .unwrap();
    }
    let jd = s
        .create_jd("Backend Dev", "Acme", "We want Rust.")
        .await
        .unwrap();
    (s, jd)
}

fn cfg() -> MatchConfig {
    MatchConfig::default()
}

#[tokio::test]
async fn skills_batched_by_128() {
    let (s, jd) = setup(1, 0, 200).await;
    let e = Recorder::new(|_| 3.0);
    run_match(&s, &e, jd, "m", cfg(), CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(e.count(is_skill_call), 2);
    let reqs = e.requests();
    assert!(reqs.iter().all(|r| r.len() <= 128));
    // seniority + role family ride in the first JD batch
    assert!(reqs[0].contains(&SENIORITY_KEY.to_string()));
    assert!(reqs[0].contains(&ROLE_FAMILY_KEY.to_string()));
    let skill_rows = s
        .scores_for_run(1)
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.target_type == TargetType::Skill)
        .count();
    assert_eq!(skill_rows, 200);
}

#[tokio::test]
async fn bullets_only_for_top_k() {
    let (s, jd) = setup(12, 2, 0).await;
    let e = Recorder::new(|_| 3.0);
    let run = run_match(&s, &e, jd, "m", cfg(), CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(e.count(is_entry_call), 12);
    assert_eq!(e.count(is_bullet_call), 8);
    assert_eq!(s.get_run(run).await.unwrap().status, RunStatus::Done);
}

fn row(t: TargetType, id: i64, q: &str, score: f64) -> ScoreRow {
    ScoreRow {
        run_id: 1,
        target_type: t,
        target_id: id,
        question: q.into(),
        probabilities: BTreeMap::new(),
        score,
    }
}

fn mk_entry(id: i64, bullet_ids: &[i64]) -> Entry {
    Entry {
        id,
        kind: EntryKind::Experience,
        title: format!("E{id}"),
        org: String::new(),
        location: String::new(),
        start_date: "2020-01".into(),
        end_date: None,
        description: String::new(),
        sort_hint: 0,
        archived: false,
        bullets: bullet_ids
            .iter()
            .enumerate()
            .map(|(i, b)| Bullet {
                id: *b,
                entry_id: id,
                text: format!("b{b}"),
                position: i as i64,
            })
            .collect(),
        skills: vec![],
    }
}

#[test]
fn rank_orders_by_score() {
    let entries = vec![mk_entry(1, &[10, 11]), mk_entry(2, &[]), mk_entry(3, &[])];
    let skills = vec![
        Skill {
            id: 5,
            name: "Rust".into(),
            category: String::new(),
        },
        Skill {
            id: 6,
            name: "Go".into(),
            category: String::new(),
        },
    ];
    let mut senior = row(TargetType::Meta, 0, SENIORITY_KEY, 0.6);
    senior.probabilities = BTreeMap::from([
        ("junior".into(), 0.1),
        ("senior".into(), 0.6),
        ("mid".into(), 0.3),
    ]);
    let rows = vec![
        row(TargetType::Entry, 1, RELEVANCE_KEY, 3.6),
        row(TargetType::Entry, 1, SENIORITY_FIT_KEY, 0.9),
        row(TargetType::Entry, 2, RELEVANCE_KEY, 0.8),
        row(TargetType::Entry, 3, RELEVANCE_KEY, 2.9),
        row(TargetType::Bullet, 10, BULLET_INSTRUCTION, 1.0),
        row(TargetType::Bullet, 11, BULLET_INSTRUCTION, 3.0),
        row(TargetType::Skill, 5, "q", 0.2),
        row(TargetType::Skill, 6, "q", 0.8),
        senior,
    ];
    let r = rank(&rows, &entries, &skills);
    let scores: Vec<f64> = r.entries.iter().map(|e| e.score).collect();
    assert_eq!(scores, vec![3.6, 2.9, 0.8]);
    let bullets: Vec<i64> = r.entries[0].bullets.iter().map(|(b, _)| b.id).collect();
    assert_eq!(bullets, vec![11, 10]);
    assert_eq!(r.skills[0].0.name, "Go");
    assert_eq!(r.seniority, "senior");
}

#[tokio::test]
async fn no_bullets_no_skills_ok() {
    let (s, jd) = setup(1, 0, 0).await;
    let e = Recorder::new(|_| 3.0);
    let run = run_match(&s, &e, jd, "m", cfg(), CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(e.count(is_skill_call), 0);
    assert_eq!(e.count(is_bullet_call), 0);
    // one JD call (seniority + role family) and one entry call
    assert_eq!(e.requests().len(), 2);
    assert_eq!(s.get_run(run).await.unwrap().status, RunStatus::Done);
}

#[tokio::test]
async fn cancel_keeps_partial() {
    let (s, jd) = setup(6, 1, 0).await;
    let e = Recorder::new(|_| 3.0);
    let cancel = CancellationToken::new();
    let c2 = cancel.clone();
    let res = run_match(&s, &e, jd, "m", cfg(), cancel, move |p| {
        if let Progress::ScoringEntries { done: 3, .. } = p {
            c2.cancel();
        }
    })
    .await;
    let run = match res {
        Err(MatchError::Cancelled { run_id }) => run_id,
        other => panic!("expected Cancelled, got {other:?}"),
    };
    assert_eq!(s.get_run(run).await.unwrap().status, RunStatus::Cancelled);
    let entry_scores = |rows: &[ScoreRow]| {
        rows.iter()
            .filter(|r| r.target_type == TargetType::Entry && r.question == RELEVANCE_KEY)
            .count()
    };
    assert_eq!(entry_scores(&s.scores_for_run(run).await.unwrap()), 3);
    assert_eq!(e.count(is_bullet_call), 0);

    let e2 = Recorder::new(|_| 3.0);
    let resumed = resume_match(&s, &e2, run, cfg(), CancellationToken::new(), |_| {})
        .await
        .unwrap();
    assert_eq!(resumed, run);
    // No JD call (meta already scored), 3 remaining entries, 6 bullet calls.
    assert_eq!(e2.count(is_entry_call), 3);
    assert_eq!(e2.count(is_bullet_call), 6);
    assert_eq!(e2.requests().len(), 9);
    // 3 entries x 2 questions + 6 bullets x 1 question
    assert_eq!(e2.inner.calls(), 12);
    let rows = s.scores_for_run(run).await.unwrap();
    assert_eq!(entry_scores(&rows), 6);
    assert_eq!(
        rows.iter()
            .filter(|r| r.target_type == TargetType::Meta)
            .count(),
        2
    );
    assert_eq!(s.get_run(run).await.unwrap().status, RunStatus::Done);
}

#[tokio::test]
async fn engine_failure_marks_failed() {
    // fail_on_call counts questions: JD batch = q1,q2; entry 1 = q3,q4 -> fails.
    let (s, jd) = setup(3, 1, 0).await;
    let e = Recorder::new(|_| 3.0);
    e.inner.fail_on_call(4);
    let res = run_match(&s, &e, jd, "m", cfg(), CancellationToken::new(), |_| {}).await;
    assert!(matches!(res, Err(MatchError::Engine(_))), "{res:?}");
    let run = 1;
    assert_eq!(s.get_run(run).await.unwrap().status, RunStatus::Failed);
    let rows = s.scores_for_run(run).await.unwrap();
    assert_eq!(
        rows.iter()
            .filter(|r| r.target_type == TargetType::Meta)
            .count(),
        2
    );
    assert!(rows.iter().all(|r| r.target_type == TargetType::Meta));
}

#[tokio::test]
async fn engine_failure_later_keeps_entry_scores() {
    // q1,q2 JD; q3..q8 entries 1-3; q9 = first bullet -> fails.
    let (s, jd) = setup(3, 1, 0).await;
    let e = Recorder::new(|_| 3.0);
    e.inner.fail_on_call(9);
    let res = run_match(&s, &e, jd, "m", cfg(), CancellationToken::new(), |_| {}).await;
    assert!(matches!(res, Err(MatchError::Engine(_))));
    assert_eq!(s.get_run(1).await.unwrap().status, RunStatus::Failed);
    let rows = s.scores_for_run(1).await.unwrap();
    assert_eq!(
        rows.iter()
            .filter(|r| r.target_type == TargetType::Entry)
            .count(),
        6
    );
}
