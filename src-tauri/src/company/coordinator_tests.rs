use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use super::agent::{ResearchError, ResearchInput, ResearchOutput, ResearchStep};
use super::coordinator::*;
use super::{format_ts, normalize_name};
use crate::store::types::{CompanyProfile, CompanyStatus};
use crate::store::Store;

type Events = Arc<Mutex<Vec<(String, Value)>>>;

fn coordinator(max: usize) -> (ResearchCoordinator, Events) {
    let events: Events = Arc::default();
    let sink = events.clone();
    let c = ResearchCoordinator::new(max, Arc::new(move |n: &str, v: Value| sink.lock().unwrap().push((n.into(), v))));
    (c, events)
}

fn stages(events: &Events, company_id: i64) -> Vec<String> {
    events
        .lock()
        .unwrap()
        .iter()
        .filter(|(n, v)| n == RESEARCH_EVENT && v["company_id"] == company_id)
        .map(|(_, v)| v["event"]["stage"].as_str().unwrap().to_string())
        .collect()
}

fn input(name: &str) -> ResearchInput {
    ResearchInput { name: name.into(), domain: None, jd_snippet: String::new() }
}

fn output(name: &str, domain: Option<&str>) -> ResearchOutput {
    ResearchOutput {
        canonical_name: name.into(),
        domain: domain.map(Into::into),
        profile: CompanyProfile { industry: "Fintech".into(), ..Default::default() },
    }
}

/// A research fn that counts calls, waits `delay`, then returns `result`.
fn scripted(calls: Arc<AtomicUsize>, delay: Duration, result: Result<ResearchOutput, ResearchError>) -> ResearchFn {
    Arc::new(move |_input, cancel: CancellationToken, on_step| {
        calls.fetch_add(1, Ordering::SeqCst);
        let result = result.clone();
        Box::pin(async move {
            on_step(ResearchStep::Round { n: 1, max: 25 });
            tokio::select! {
                _ = cancel.cancelled() => Err(ResearchError::Cancelled),
                _ = tokio::time::sleep(delay) => result.map(|o| (o, "lfm".to_string())),
            }
        })
    })
}

async fn store_with(name: &str) -> (Store, i64) {
    let s = Store::open("sqlite::memory:").await.unwrap();
    let c = s.create_company(name, &normalize_name(name), None).await.unwrap();
    (s, c.id)
}

#[tokio::test]
async fn fresh_company_skips_research() {
    let (s, id) = store_with("Acme").await;
    s.save_research(id, "Acme", None, &CompanyProfile::default(), "m").await.unwrap();
    let (c, _) = coordinator(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let got = c.ensure(&s, s.get_company(id).await.unwrap(), input("Acme"), scripted(calls.clone(), Duration::ZERO, Ok(output("Acme", None)))).await.unwrap();
    assert_eq!(got.status, CompanyStatus::Ok);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn concurrent_ensure_runs_once() {
    let (s, id) = store_with("Acme").await;
    let (c, events) = coordinator(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let f = scripted(calls.clone(), Duration::from_millis(50), Ok(output("Acme Inc", Some("acme.com"))));
    let co = s.get_company(id).await.unwrap();
    let (a, b) = tokio::join!(c.ensure(&s, co.clone(), input("Acme"), f.clone()), c.ensure(&s, co, input("Acme"), f));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(a.unwrap().profile.industry, "Fintech");
    assert_eq!(b.unwrap().name, "Acme Inc");
    assert_eq!(stages(&events, id), vec!["queued", "started", "step", "ready"]);
}

#[tokio::test]
async fn concurrency_cap_queues_second_company() {
    let s = Store::open("sqlite::memory:").await.unwrap();
    let a = s.create_company("A", "a", None).await.unwrap();
    let b = s.create_company("B", "b", None).await.unwrap();
    let (c, events) = coordinator(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let f = scripted(calls.clone(), Duration::from_millis(80), Ok(output("X", None)));
    let fa = c.ensure(&s, a.clone(), input("A"), f.clone());
    let fb = async {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let before = stages(&events, b.id);
        let r = c.ensure(&s, b.clone(), input("B"), f.clone()).await;
        (before, r)
    };
    let (ra, (_, rb)) = tokio::join!(fa, fb);
    ra.unwrap();
    rb.unwrap();
    // B was queued while A held the only slot, so B started after A was ready.
    let all: Vec<(i64, String)> = events
        .lock()
        .unwrap()
        .iter()
        .map(|(_, v)| (v["company_id"].as_i64().unwrap(), v["event"]["stage"].as_str().unwrap().to_string()))
        .collect();
    let pos = |id: i64, st: &str| all.iter().position(|(i, s)| *i == id && s == st).unwrap();
    assert!(pos(b.id, "queued") < pos(a.id, "ready"));
    assert!(pos(b.id, "started") > pos(a.id, "ready"));
}

#[tokio::test]
async fn failure_sets_backoff() {
    let (s, id) = store_with("Acme").await;
    let (c, events) = coordinator(1);
    let f = scripted(Arc::default(), Duration::ZERO, Err(ResearchError::Budget));
    let err = c.ensure(&s, s.get_company(id).await.unwrap(), input("Acme"), f).await.unwrap_err();
    assert_eq!(err, ResearchError::Budget);
    let got = s.get_company(id).await.unwrap();
    assert_eq!(got.status, CompanyStatus::Failed);
    assert_eq!(got.fail_count, 1);
    assert!(got.retry_after.unwrap() > format_ts(chrono::Utc::now()));
    assert_eq!(stages(&events, id).last().unwrap(), "failed");
}

#[tokio::test]
async fn unavailable_does_not_back_off() {
    let (s, id) = store_with("Acme").await;
    let (c, _) = coordinator(1);
    let f = scripted(Arc::default(), Duration::ZERO, Err(ResearchError::Unavailable("Add your Exa key in AI → Providers.".into())));
    let err = c.ensure(&s, s.get_company(id).await.unwrap(), input("Acme"), f).await.unwrap_err();
    assert_eq!(err.to_string(), "Add your Exa key in AI → Providers.");
    let got = s.get_company(id).await.unwrap();
    assert_eq!(got.fail_count, 0);
    assert_eq!(got.retry_after, None);
}

#[tokio::test]
async fn cancel_leaves_row_unchanged() {
    let (s, id) = store_with("Acme").await;
    let (c, events) = coordinator(1);
    let f = scripted(Arc::default(), Duration::from_secs(30), Ok(output("Acme", None)));
    let co = s.get_company(id).await.unwrap();
    let (r, _) = tokio::join!(c.ensure(&s, co, input("Acme"), f), async {
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(c.cancel(id));
    });
    assert_eq!(r.unwrap_err(), ResearchError::Cancelled);
    let got = s.get_company(id).await.unwrap();
    assert_eq!(got.status, CompanyStatus::Pending);
    assert_eq!(got.fail_count, 0);
    assert_eq!(stages(&events, id).last().unwrap(), "cancelled");
    assert!(!c.cancel(id));
}

#[tokio::test]
async fn waiter_dropped_research_still_saves() {
    let (s, id) = store_with("Acme").await;
    let (c, _) = coordinator(1);
    let f = scripted(Arc::default(), Duration::from_millis(50), Ok(output("Acme", None)));
    let co = s.get_company(id).await.unwrap();
    let _ = tokio::time::timeout(Duration::from_millis(10), c.ensure(&s, co, input("Acme"), f)).await;
    for _ in 0..100 {
        if s.get_company(id).await.unwrap().status == CompanyStatus::Ok {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("research was not saved after the waiter left");
}

#[tokio::test]
async fn merge_on_canonical_match() {
    let s = Store::open("sqlite::memory:").await.unwrap();
    let old = s.create_company("Google", "google", Some("google.com")).await.unwrap();
    let new = s.create_company("Google Mexico", "google mexico", None).await.unwrap();
    let (c, _) = coordinator(1);
    let f = scripted(Arc::default(), Duration::ZERO, Ok(output("Google", Some("google.com"))));
    let got = c.ensure(&s, new.clone(), input("Google Mexico"), f).await.unwrap();
    assert_eq!(got.id, old.id);
    assert!(s.get_company(new.id).await.is_err());
    assert_eq!(s.find_company(None, "google mexico").await.unwrap().unwrap().id, old.id);
}

#[tokio::test]
async fn domain_only_match_does_not_merge() {
    let s = Store::open("sqlite::memory:").await.unwrap();
    let google = s.create_company("Google", "google", Some("google.com")).await.unwrap();
    let sub = s.create_company("Alphabet Mexico", "alphabet mexico", None).await.unwrap();
    let (c, _) = coordinator(1);
    let f = scripted(Arc::default(), Duration::ZERO, Ok(output("Alphabet Mexico", Some("google.com"))));
    let got = c.ensure(&s, sub.clone(), input("Alphabet Mexico"), f).await.unwrap();
    assert_eq!(got.id, sub.id, "a shared domain alone must not merge companies");
    assert_eq!(got.domain, None, "the domain stays with the row that owns it");
    assert!(s.get_company(google.id).await.is_ok());
}

#[tokio::test]
async fn job_board_domain_is_dropped() {
    let (s, id) = store_with("Acme").await;
    let (c, _) = coordinator(1);
    let f = scripted(Arc::default(), Duration::ZERO, Ok(output("Acme", Some("linkedin.com"))));
    let got = c.ensure(&s, s.get_company(id).await.unwrap(), input("Acme"), f).await.unwrap();
    assert_eq!(got.domain, None);
}

#[tokio::test]
async fn row_merged_away_mid_research_still_saves() {
    let s = Store::open("sqlite::memory:").await.unwrap();
    let nu = s.create_company("Nu", "nu", None).await.unwrap();
    let nubank = s.create_company("Nubank", "nubank", None).await.unwrap();
    let (c, _) = coordinator(2);
    let f = scripted(Arc::default(), Duration::from_millis(50), Ok(output("Nubank", Some("nubank.com.br"))));
    let (r, _) = tokio::join!(c.ensure(&s, nu.clone(), input("Nu"), f), async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        s.merge_company(nu.id, nubank.id).await.unwrap();
    });
    let got = r.expect("research survives its row being merged away");
    assert_eq!(got.id, nubank.id);
    assert_eq!(got.status, CompanyStatus::Ok);
}

#[tokio::test]
async fn concurrent_persists_for_the_same_company_do_not_fail() {
    let s = Store::open("sqlite::memory:").await.unwrap();
    let a = s.create_company("Nu", "nu", None).await.unwrap();
    let b = s.create_company("Nu Pagamentos", "nu pagamentos", None).await.unwrap();
    let (c, _) = coordinator(2);
    let f = scripted(Arc::default(), Duration::from_millis(20), Ok(output("Nubank", Some("nubank.com.br"))));
    let (ra, rb) = tokio::join!(c.ensure(&s, a.clone(), input("Nu"), f.clone()), c.ensure(&s, b.clone(), input("Nu Pagamentos"), f));
    let (ra, rb) = (ra.unwrap(), rb.unwrap());
    assert_eq!(ra.id, rb.id, "both runs end on one merged row");
    let left = s.find_company(None, "nubank").await.unwrap().unwrap();
    assert_eq!(left.fail_count, 0);
}
