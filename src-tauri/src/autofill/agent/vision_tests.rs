use super::*;
use crate::engine::FakeDecisionEngine;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn choice(c: &str, p: f64) -> Answer {
    Answer::Choice {
        choice: c.into(),
        probabilities: BTreeMap::from([(c.to_string(), p)]),
    }
}

/// Answers the given digits in order.
fn answering(seq: &'static [(&'static str, f64)]) -> FakeDecisionEngine {
    let n = Arc::new(AtomicUsize::new(0));
    FakeDecisionEngine::new(move |_, _, _| {
        let (c, p) = seq[n.fetch_add(1, Ordering::SeqCst).min(seq.len() - 1)];
        choice(c, p)
    })
    .seeing()
}

#[tokio::test]
async fn probe_passes_when_both_digits_are_named() {
    let e = answering(&[("3", 0.9), ("8", 0.8)]);
    assert_eq!(probe(&e, [3, 8]).await, Ok(true));
}

#[tokio::test]
async fn probe_fails_on_a_wrong_or_unsure_digit() {
    assert_eq!(probe(&answering(&[("3", 0.9), ("2", 0.9)]), [3, 8]).await, Ok(false));
    assert_eq!(probe(&answering(&[("3", 0.3)]), [3, 8]).await, Ok(false));
}

#[tokio::test]
async fn text_only_engines_fail_the_probe() {
    let e = FakeDecisionEngine::new(|_, _, _| choice("3", 1.0));
    assert_eq!(probe(&e, [3, 8]).await, Ok(false));
}

#[tokio::test]
async fn engine_errors_pass_through() {
    let e = answering(&[("3", 0.9)]);
    e.fail_on_call(1);
    assert!(probe(&e, [3, 8]).await.is_err());
}

#[test]
fn random_digits_differ() {
    for _ in 0..50 {
        let [a, b] = random_digits();
        assert!(a < 10 && b < 10 && a != b);
    }
}

#[test]
fn digit_images_are_pngs_and_differ() {
    let decode = |s: &str| base64::engine::general_purpose::STANDARD.decode(s).unwrap();
    let one = decode(&digit_png(1));
    assert_eq!(&one[1..4], b"PNG");
    assert_ne!(one, decode(&digit_png(8)));
}

#[tokio::test]
async fn result_is_cached_per_model() {
    let st = test_state().await;
    let e = answering(&[("x", 0.9)]);
    assert!(!supports_images(&st, "typesafe:jev-latest", &e).await);
    assert_eq!(e.calls(), 1);
    assert!(!supports_images(&st, "typesafe:jev-latest", &e).await);
    assert_eq!(e.calls(), 1);
    st.store.set_setting(&cache_key("typesafe:jev-preview"), "true").await.unwrap();
    assert!(supports_images(&st, "typesafe:jev-preview", &e).await);
}

pub(crate) async fn test_state() -> AppState {
    use crate::state::{EngineSource, Emitter};
    let store = crate::store::Store::open("sqlite::memory:").await.unwrap();
    let emit: Emitter = Arc::new(|_: &str, _: serde_json::Value| {});
    let fake = FakeDecisionEngine::new(|_, _, _| Answer::Noul { p: 0.5 });
    AppState::new(store, EngineSource::Fixed(Arc::new(fake)), emit)
}
