use super::*;
use crate::engine::{parse_wire_response, Question};
use std::collections::BTreeMap;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../tests/fixtures/openrouter_systemone_response.json")).unwrap()
}

fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([("q".to_string(), Question::Noul { instructions: "Rust?".into() })])
}

fn engine(server: &MockServer) -> HttpSystemOneEngine {
    HttpSystemOneEngine::new(server.uri(), "k".into(), "m".into())
}

async fn status_error(status: u16, body: serde_json::Value) -> EngineError {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    engine(&server).decide("s", &questions()).await.unwrap_err()
}

#[tokio::test]
async fn sends_model_and_bearer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer k"))
        .and(body_partial_json(serde_json::json!({"model": "m", "state": "s"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture()))
        .mount(&server)
        .await;
    let got = engine(&server).decide("s", &questions()).await.unwrap();
    assert_eq!(got, parse_wire_response(fixture()).unwrap());
}

#[tokio::test]
async fn maps_status_codes() {
    let err = |m: &str| serde_json::json!({"error": {"message": m}});
    assert_eq!(status_error(401, err("User not found.")).await, EngineError::Unauthorized);
    assert_eq!(status_error(402, err("Insufficient credits")).await, EngineError::InsufficientCredits);
    assert!(matches!(status_error(404, err("not found")).await, EngineError::ModelUnavailable(_)));
    assert!(matches!(
        status_error(400, err("Model nope/nope does not exist")).await,
        EngineError::ModelUnavailable(_)
    ));
    assert!(matches!(status_error(400, err("questions invalid")).await, EngineError::BadResponse(_)));
}

#[tokio::test]
async fn retries_429_once() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture()))
        .mount(&server)
        .await;
    assert!(engine(&server).decide("s", &questions()).await.is_ok());
}

#[tokio::test]
async fn gives_up_after_second_429() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .expect(2)
        .mount(&server)
        .await;
    assert_eq!(engine(&server).decide("s", &questions()).await.unwrap_err(), EngineError::RateLimited);
}

#[tokio::test]
async fn bad_body_is_bad_response() {
    assert!(matches!(status_error(200, serde_json::json!({})).await, EngineError::BadResponse(_)));
}

#[tokio::test]
async fn network_error() {
    let e = HttpSystemOneEngine::new("http://127.0.0.1:1".into(), "k".into(), "m".into());
    assert!(matches!(e.decide("s", &questions()).await.unwrap_err(), EngineError::Network(_)));
}
