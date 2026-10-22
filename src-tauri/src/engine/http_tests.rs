use super::*;
use crate::engine::{parse_wire_response, Question};
use std::collections::BTreeMap;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/openrouter_systemone_response.json"
    ))
    .unwrap()
}

fn questions() -> BTreeMap<String, Question> {
    BTreeMap::from([(
        "q".to_string(),
        Question::Noul {
            instructions: "Rust?".into(),
        },
    )])
}

fn engine(server: &MockServer) -> HttpSystemOneEngine {
    HttpSystemOneEngine::new("OpenRouter", server.uri(), "k".into(), "m".into())
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
        .and(body_partial_json(
            serde_json::json!({"model": "m", "state": "s"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture()))
        .mount(&server)
        .await;
    let got = engine(&server).decide("s", &questions()).await.unwrap();
    assert_eq!(got, parse_wire_response(fixture()).unwrap());
}

#[tokio::test]
async fn maps_status_codes() {
    let err = |m: &str| serde_json::json!({"error": {"message": m}});
    assert_eq!(
        status_error(401, err("User not found.")).await,
        EngineError::Unauthorized("OpenRouter")
    );
    assert_eq!(
        status_error(402, err("Insufficient credits")).await,
        EngineError::InsufficientCredits("OpenRouter")
    );
    assert!(matches!(
        status_error(404, err("not found")).await,
        EngineError::ModelUnavailable(_)
    ));
    assert!(matches!(
        status_error(400, err("Model nope/nope does not exist")).await,
        EngineError::ModelUnavailable(_)
    ));
    assert!(matches!(
        status_error(400, err("questions invalid")).await,
        EngineError::BadResponse(_)
    ));
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
    assert_eq!(
        engine(&server).decide("s", &questions()).await.unwrap_err(),
        EngineError::RateLimited
    );
}

#[tokio::test]
async fn bad_body_is_bad_response() {
    assert!(matches!(
        status_error(200, serde_json::json!({})).await,
        EngineError::BadResponse(_)
    ));
}

#[tokio::test]
async fn network_error() {
    let e = HttpSystemOneEngine::new(
        "OpenRouter",
        "http://127.0.0.1:1".into(),
        "k".into(),
        "m".into(),
    );
    assert!(matches!(
        e.decide("s", &questions()).await.unwrap_err(),
        EngineError::Network(_)
    ));
}

#[tokio::test]
async fn subq_error_detail_message_surfaces() {
    let err = status_error(422, serde_json::json!({"detail": {"error_type": "validation_error", "message": "questions/queue: must have required property 'criteria'"}})).await;
    assert_eq!(err, EngineError::BadResponse("HTTP 422 Unprocessable Entity: questions/queue: must have required property 'criteria'".into()));
}

#[tokio::test]
async fn subq_token_limit_is_context_overflow() {
    let err = status_error(400, serde_json::json!({"detail": {"error_type": "validation_error", "message": "input is over the token limit"}})).await;
    assert_eq!(err, EngineError::ContextOverflow);
}

#[tokio::test]
async fn max_tokens_exceeded_is_context_overflow() {
    let e = status_error(
        400,
        serde_json::json!({"code": "max_tokens_exceeded", "message": "request too long"}),
    )
    .await;
    assert_eq!(e, EngineError::ContextOverflow);
}

#[tokio::test]
async fn overloaded_529_is_retried_once_then_rate_limited() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(529))
        .mount(&server)
        .await;
    let err = engine(&server).decide("s", &questions()).await.unwrap_err();
    assert_eq!(err, EngineError::RateLimited);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

fn image() -> Vec<crate::engine::Image> {
    vec![crate::engine::Image {
        placeholder: "img-1".into(),
        png_base64: "AAAA".into(),
    }]
}

#[tokio::test]
async fn image_requests_send_assets_and_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("TypeSafe-Danger-Zone-Images", "true"))
        .and(body_partial_json(serde_json::json!({
            "assets": {"img-1": {"type": "image", "format": "png", "data_base64": "AAAA"}}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture()))
        .mount(&server)
        .await;
    let got = engine(&server)
        .decide_with_images("s img-1", &questions(), &image())
        .await
        .unwrap();
    assert_eq!(got, parse_wire_response(fixture()).unwrap());
}

#[tokio::test]
async fn text_requests_have_no_assets_or_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture()))
        .mount(&server)
        .await;
    engine(&server).decide("s", &questions()).await.unwrap();
    let req = &server.received_requests().await.unwrap()[0];
    assert!(req.headers.get("TypeSafe-Danger-Zone-Images").is_none());
    let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
    assert!(body.get("assets").is_none());
}

#[tokio::test]
async fn rejected_images_mean_no_vision() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(serde_json::json!({"error": {"message": "images not supported"}})),
        )
        .mount(&server)
        .await;
    let err = engine(&server)
        .decide_with_images("s", &questions(), &image())
        .await
        .unwrap_err();
    assert_eq!(err, EngineError::ImagesUnsupported);
}
