use serde_json::{json, Value};
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::providers::ProviderError;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/exa_search_response.json"
    ))
    .unwrap()
}

#[tokio::test]
async fn exa_parses_fixture() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture()))
        .mount(&server)
        .await;
    let hits = ExaSearch::new(server.uri(), "k".into())
        .search("anthropic company", 5)
        .await
        .unwrap();
    assert_eq!(hits.len(), 5);
    assert_eq!(hits[0].url, "https://www.anthropic.com/");
    assert_eq!(hits[0].title, "Home \\ Anthropic");
    assert!(hits[0].snippet.contains("Anthropic"));
}

#[tokio::test]
async fn exa_sends_api_key_header_and_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/search"))
        .and(header("x-api-key", "secret"))
        .and(body_partial_json(
            json!({"query": "acme", "numResults": 5, "contents": {"text": {"maxCharacters": 500}}}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"results": []})))
        .expect(1)
        .mount(&server)
        .await;
    let hits = ExaSearch::new(server.uri(), "secret".into())
        .search("acme", 5)
        .await
        .unwrap();
    assert!(hits.is_empty());
}

#[tokio::test]
async fn exa_401_is_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let err = ExaSearch::new(server.uri(), "bad".into())
        .search("acme", 5)
        .await
        .unwrap_err();
    assert_eq!(err, ProviderError::KeyRejected("Exa"));
    assert_eq!(err.to_string(), "Check your Exa key");
}

#[tokio::test]
async fn exa_429_is_network_error_with_rate_limit_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let err = ExaSearch::new(server.uri(), "k".into())
        .search("acme", 5)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ProviderError::Network(m) if m.contains("rate limit")),
        "{err:?}"
    );
}

#[tokio::test]
async fn exa_missing_results_is_bad_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"error": "x"})))
        .mount(&server)
        .await;
    let err = ExaSearch::new(server.uri(), "k".into())
        .search("acme", 5)
        .await
        .unwrap_err();
    assert!(matches!(err, ProviderError::BadResponse(_)));
}
