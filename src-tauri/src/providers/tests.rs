use super::*;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn models_fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../tests/fixtures/openrouter_models.json")).unwrap()
}

#[test]
fn model_ref_parse() {
    let local = ModelRef::parse("clef-flash");
    assert_eq!((local.provider.as_str(), local.id.as_str()), ("local", "clef-flash"));
    let remote = ModelRef::parse("openrouter:inception/mercury-decide:free");
    assert_eq!((remote.provider.as_str(), remote.id.as_str()), ("openrouter", "inception/mercury-decide:free"));
    assert_eq!(local.encode(), "clef-flash");
    assert_eq!(remote.encode(), "openrouter:inception/mercury-decide:free");
}

#[tokio::test]
async fn openrouter_lists_decision_models() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(query_param("output_modalities", "decisions"))
        .and(header("authorization", "Bearer k"))
        .respond_with(ResponseTemplate::new(200).set_body_json(models_fixture()))
        .mount(&server)
        .await;
    let models = OpenRouterProvider::new(server.uri(), "k".into()).list_models().await.unwrap();
    let jev = models.iter().find(|m| m.id == "typesafe/jev-1.13").unwrap();
    assert_eq!(jev.provider, "openrouter");
    assert_eq!(jev.context_length, Some(64000));
    let price = jev.price_per_mtok_in.unwrap();
    assert!((price - 0.042).abs() < 1e-9, "{price}");
    assert_eq!(models.len(), 4);
}

#[tokio::test]
async fn openrouter_401_is_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/key"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({"error": {"message": "User not found.", "code": 401}})))
        .mount(&server)
        .await;
    let err = OpenRouterProvider::new(server.uri(), "bad".into()).validate_key().await.unwrap_err();
    assert_eq!(err, ProviderError::Unauthorized);
}
