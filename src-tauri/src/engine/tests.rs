use super::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn fixture(name: &str) -> Value {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

#[test]
fn parses_blog_example() {
    let got = parse_wire_response(fixture("blog_response.json")).unwrap();
    assert_eq!(got["angry"], Answer::Noul { p: 0.8208 });
    match &got["urgency"] {
        Answer::Score { score, probabilities } => {
            assert_eq!(*score, 2.2821);
            assert_eq!(probabilities["2"], 0.4);
        }
        other => panic!("{other:?}"),
    }
    match &got["route"] {
        Answer::Choice { choice, probabilities } => {
            assert_eq!(choice, "billing");
            assert_eq!(probabilities.len(), 3);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn wire_request_matches_spike() {
    let fx = fixture("decision_request.json");
    let state = fx["state"].as_str().unwrap();
    let legend: BTreeMap<u8, String> = [
        "irrelevant to this role",
        "weakly related",
        "somewhat relevant",
        "clearly relevant",
        "highly relevant, a core match",
    ]
    .iter()
    .enumerate()
    .map(|(i, s)| (i as u8, s.to_string()))
    .collect();
    let mut qs = BTreeMap::new();
    qs.insert("q_noul".to_string(), Question::Noul { instructions: "Does this role require or value Rust?".into() });
    qs.insert(
        "q_choice".to_string(),
        Question::Choice {
            instructions: "Which area best describes this CV entry?".into(),
            options: ["backend", "frontend", "data_ml", "other"].map(String::from).to_vec(),
        },
    );
    qs.insert(
        "q_score".to_string(),
        Question::Score { instructions: "How relevant is this CV entry to the job description above?".into(), legend },
    );
    assert_eq!(wire_request(state, &qs), fx);
}

#[tokio::test]
async fn rejects_129_questions() {
    let fake = FakeDecisionEngine::new(|_, _, _| Answer::Noul { p: 0.5 });
    let mut qs = BTreeMap::new();
    for i in 0..129 {
        qs.insert(format!("q{i:03}"), Question::Noul { instructions: "x".into() });
    }
    assert_eq!(decide_checked(&fake, "s", &qs).await, Err(EngineError::TooManyQuestions { max: 128 }));
    assert_eq!(fake.calls(), 0);
    qs.remove("q000");
    assert_eq!(decide_checked(&fake, "s", &qs).await.unwrap().len(), 128);
}

#[tokio::test]
async fn provider_cap_is_enforced_before_the_request() {
    let engine = HttpSystemOneEngine::new("SubQ", "http://127.0.0.1:1".into(), "k".into(), "m".into())
        .with_max_questions(Some(16));
    let qs = |n: usize| -> BTreeMap<String, Question> {
        (0..n).map(|i| (format!("q{i:03}"), Question::Noul { instructions: "x".into() })).collect()
    };
    assert_eq!(decide_checked(&engine, "s", &qs(17)).await, Err(EngineError::TooManyQuestions { max: 16 }));
    // 16 is within the cap, so it reaches the (unreachable) network.
    assert!(matches!(decide_checked(&engine, "s", &qs(16)).await, Err(EngineError::Network(_))));
}

#[test]
fn context_overflow_mapped() {
    let body = json!({"error":{"code":500,"message":"input (1921 tokens) is too large to process. increase the physical batch size (current batch size: 512)","type":"server_error"}});
    assert_eq!(parse_wire_response(body), Err(EngineError::ContextOverflow));
    let other = json!({"error":{"code":400,"message":"nope","type":"invalid_request_error"}});
    assert!(matches!(parse_wire_response(other), Err(EngineError::BadResponse(_))));
}

#[tokio::test]
async fn fake_counts_and_fails_on_nth() {
    let fake = FakeDecisionEngine::new(|_, _, _| Answer::Noul { p: 0.1 });
    fake.fail_on_call(2);
    let mut qs = BTreeMap::new();
    qs.insert("a".to_string(), Question::Noul { instructions: "x".into() });
    qs.insert("b".to_string(), Question::Noul { instructions: "y".into() });
    assert!(matches!(fake.decide("s", &qs).await, Err(EngineError::Unavailable(_))));
    assert_eq!(fake.calls(), 2);
}

#[test]
fn provider_cap_is_clamped_to_valid_range() {
    let engine = |cap| {
        HttpSystemOneEngine::new("SubQ", "http://127.0.0.1:1".into(), "k".into(), "m".into())
            .with_max_questions(cap)
    };
    assert_eq!(engine(Some(0)).max_questions(), 1);
    assert_eq!(engine(Some(500)).max_questions(), MAX_QUESTIONS);
    assert_eq!(engine(None).max_questions(), MAX_QUESTIONS);
}
