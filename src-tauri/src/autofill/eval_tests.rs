//! Offline eval of the autofill harness on saved forms (`fixtures/*.json`).
//!
//! Each fixture holds the `FieldSpec`s the injected script sent for a real page,
//! each with `expect`: the fact key that belongs there, `"resume"`, `null` when
//! cvd should leave it to the user, or a list of acceptable answers (which may
//! include `null`). The facts come from a fixed synthetic
//! profile, never the user's.
//!
//! - `eval_rules_only` runs in CI: rules alone must never fill a field wrongly.
//! - `eval_live` (ignored) adds a real Fast Model:
//!   `CVD_EVAL_TYPESAFE_KEY=… [CVD_EVAL_MODEL=jev-latest] cargo test --lib eval_live -- --ignored --nocapture`

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;

use super::field::{FieldSpec, FillAction};
use super::window::{plan, Resume, Session};
use super::{build_facts, ApplyDefaults};
use crate::engine::DecisionEngine;
use crate::store::types::{Entry, EntryKind, Profile};

#[derive(Deserialize)]
struct Fixture {
    url: String,
    fields: Vec<ExpectedField>,
}

#[derive(Deserialize)]
struct ExpectedField {
    #[serde(flatten)]
    spec: serde_json::Value,
    #[serde(default)]
    expect: serde_json::Value,
}

impl ExpectedField {
    /// Acceptable answers; `None` means "left to the user".
    fn accepted(&self) -> Vec<Option<String>> {
        let one = |v: &serde_json::Value| v.as_str().map(str::to_string);
        match &self.expect {
            serde_json::Value::Array(xs) => xs.iter().map(one).collect(),
            v => vec![one(v)],
        }
    }
}

fn fixtures() -> Vec<(String, Fixture)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/autofill/fixtures");
    let mut out: Vec<(String, Fixture)> = std::fs::read_dir(&dir)
        .expect("fixtures dir")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| {
            let name = p.file_stem().unwrap().to_string_lossy().into_owned();
            let f: Fixture = serde_json::from_str(&std::fs::read_to_string(&p).unwrap())
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            (name, f)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(!out.is_empty(), "no fixtures in {}", dir.display());
    out
}

/// A complete synthetic applicant, so every fact exists.
fn eval_facts() -> Vec<super::Fact> {
    let profile = Profile {
        name: "Ana María López Ruiz".into(),
        headline: String::new(),
        email: "ana@example.com".into(),
        phone: "+34 612 345 678".into(),
        location: "Madrid, Spain".into(),
        links: vec![
            "linkedin.com/in/ana-lopez".into(),
            "github.com/analopez".into(),
            "analopez.dev".into(),
        ],
        summary: String::new(),
    };
    let entry = Entry {
        id: 1,
        kind: EntryKind::Experience,
        title: "Senior Software Engineer".into(),
        org: "Globex".into(),
        location: String::new(),
        start_date: "2019-03".into(),
        end_date: None,
        description: String::new(),
        sort_hint: 0,
        archived: false,
        bullets: vec![],
        skills: vec![],
    };
    let defaults = ApplyDefaults {
        city: "Madrid".into(),
        region: "Madrid".into(),
        country: "Spain".into(),
        authorized_to_work: Some(true),
        needs_sponsorship: Some(true),
        willing_to_relocate: Some(true),
        notice_period: "1 month".into(),
        earliest_start: "In one month".into(),
        ..Default::default()
    };
    build_facts(&profile, &[entry], &defaults, (2026, 10))
}

#[derive(Default, Debug)]
struct Score {
    correct: usize,
    wrong: usize,
    abstained: usize,
    missed: usize,
}

impl Score {
    fn add(&mut self, o: &Score) {
        self.correct += o.correct;
        self.wrong += o.wrong;
        self.abstained += o.abstained;
        self.missed += o.missed;
    }

    fn fillable(&self) -> usize {
        self.correct + self.wrong + self.missed
    }

    fn line(&self) -> String {
        let pct = |n: usize, d: usize| {
            if d == 0 {
                0.0
            } else {
                100.0 * n as f64 / d as f64
            }
        };
        format!(
            "correct {}/{} ({:.0}%), wrong {} ({:.1}% of fills), missed {}, rightly left {}",
            self.correct,
            self.fillable(),
            pct(self.correct, self.fillable()),
            self.wrong,
            pct(self.wrong, self.correct + self.wrong),
            self.missed,
            self.abstained,
        )
    }
}

/// The ATS a fixture came from: the URL's host, or the host in parentheses
/// for synthetic fixtures, with tenant subdomains folded (`acme.wd5.myworkdayjobs.com`
/// → `myworkdayjobs.com`).
fn host_of(url: &str) -> String {
    let raw = if let Some(rest) = url.strip_prefix("synthetic:") {
        match (rest.rfind('('), rest.rfind(')')) {
            (Some(a), Some(b)) if a < b => rest[a + 1..b].trim().to_string(),
            _ => return "synthetic".into(),
        }
    } else {
        tauri::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| "unknown".into())
    };
    let parts: Vec<&str> = raw.split('.').collect();
    let keep = if parts.len() > 2
        && ["greenhouse", "myworkdayjobs", "smartrecruiters"].contains(&parts[parts.len() - 2])
    {
        2
    } else {
        parts.len().min(3)
    };
    parts[parts.len() - keep..].join(".")
}

#[test]
fn hosts_group_tenants() {
    assert_eq!(
        host_of("https://job-boards.greenhouse.io/anthropic/jobs/1"),
        "greenhouse.io"
    );
    assert_eq!(
        host_of("synthetic: Workday-style (acme.wd5.myworkdayjobs.com)"),
        "myworkdayjobs.com"
    );
    assert_eq!(host_of("https://jobs.lever.co/palantir/1"), "jobs.lever.co");
    assert_eq!(host_of("synthetic: no host"), "synthetic");
}

async fn run(engine: Option<&dyn DecisionEngine>) -> Score {
    let facts = eval_facts();
    let resume = Resume {
        name: "cv.pdf".into(),
        b64: String::new(),
    };
    let mut total = Score::default();
    let mut by_host: BTreeMap<String, Score> = BTreeMap::new();
    for (name, fx) in fixtures() {
        let specs: Vec<FieldSpec> = fx
            .fields
            .iter()
            .map(|f| serde_json::from_value(f.spec.clone()).unwrap())
            .collect();
        let expected: BTreeMap<String, Vec<Option<String>>> = specs
            .iter()
            .zip(&fx.fields)
            .map(|(s, f)| (s.key.clone(), f.accepted()))
            .collect();
        let actions = plan(
            specs.clone(),
            &facts,
            Some(&resume),
            engine,
            &mut Session::default(),
        )
        .await;
        let mut score = Score::default();
        for a in &actions {
            let got = match a {
                FillAction::Set { fact, .. } => Some(fact.to_string()),
                FillAction::File { .. } => Some("resume".to_string()),
                FillAction::NeedsUser { .. } => None,
            };
            let want = expected.get(a.key()).cloned().unwrap_or_else(|| vec![None]);
            let label = specs
                .iter()
                .find(|s| s.key == a.key())
                .map(|s| s.label.as_str())
                .unwrap_or("");
            match &got {
                Some(_) if want.contains(&got) => score.correct += 1,
                Some(g) => {
                    score.wrong += 1;
                    println!("  WRONG  {name} {:?}: filled {g}, expected {want:?}", label);
                }
                None if want.contains(&None) => score.abstained += 1,
                None => {
                    score.missed += 1;
                    let why = match a {
                        FillAction::NeedsUser { reason, .. } => reason.as_str(),
                        _ => "",
                    };
                    println!("  missed {name} {:?}: expected {want:?} ({why})", label);
                }
            }
        }
        println!("{name} ({}): {}", fx.url, score.line());
        by_host.entry(host_of(&fx.url)).or_default().add(&score);
        total.add(&score);
    }
    for (host, score) in &by_host {
        println!("HOST {host}: {}", score.line());
    }
    println!("TOTAL: {}", total.line());
    total
}

#[tokio::test]
async fn eval_rules_only() {
    let s = run(None).await;
    assert_eq!(
        s.wrong,
        0,
        "rules must never fill a field wrongly: {}",
        s.line()
    );
}

/// Prints every question and answer when `CVD_EVAL_VERBOSE` is set.
struct Logged(std::sync::Arc<dyn DecisionEngine>);

#[async_trait::async_trait]
impl DecisionEngine for Logged {
    fn max_questions(&self) -> usize {
        self.0.max_questions()
    }

    async fn decide(
        &self,
        state: &str,
        questions: &BTreeMap<String, crate::engine::Question>,
    ) -> Result<BTreeMap<String, crate::engine::Answer>, crate::engine::EngineError> {
        let answers = self.0.decide(state, questions).await?;
        if std::env::var("CVD_EVAL_VERBOSE").is_ok() {
            for (id, q) in questions {
                let crate::engine::Question::Choice { instructions, .. } = q else {
                    continue;
                };
                let Some(crate::engine::Answer::Choice {
                    choice,
                    probabilities,
                }) = answers.get(id)
                else {
                    continue;
                };
                let opts = &instructions["options"];
                let mut probs: Vec<(String, f64)> = probabilities
                    .iter()
                    .map(|(k, p)| {
                        (
                            opts.get(k)
                                .and_then(|v| v.as_str())
                                .unwrap_or(k)
                                .to_string(),
                            *p,
                        )
                    })
                    .collect();
                probs.sort_by(|a, b| b.1.total_cmp(&a.1));
                let top: Vec<String> = probs
                    .iter()
                    .take(3)
                    .map(|(t, p)| format!("{t:.40} {p:.2}"))
                    .collect();
                println!(
                    "    Q {:?} -> {choice} | {}",
                    instructions["field"]["label"].as_str().unwrap_or(""),
                    top.join(" ; ")
                );
            }
        }
        Ok(answers)
    }
}

#[tokio::test]
#[ignore = "calls a real Fast Model; needs CVD_EVAL_TYPESAFE_KEY"]
async fn eval_live() {
    use crate::providers::{Provider, TypeSafeProvider, TYPESAFE_BASE};
    let key = std::env::var("CVD_EVAL_TYPESAFE_KEY").expect("set CVD_EVAL_TYPESAFE_KEY");
    let model = std::env::var("CVD_EVAL_MODEL").unwrap_or_else(|_| "jev-latest".into());
    let engine = Logged(
        TypeSafeProvider::new(TYPESAFE_BASE.into(), key)
            .engine(&model)
            .unwrap(),
    );
    println!("model: typesafe:{model}");
    let s = run(Some(&engine)).await;
    // Release gate from the spec: wrong fills ≤ 2 % of fills.
    assert!(
        s.wrong * 50 <= s.correct + s.wrong,
        "too many wrong fills: {}",
        s.line()
    );
}
