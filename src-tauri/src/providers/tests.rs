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

async fn keyed_store() -> crate::store::Store {
    let s = crate::store::Store::open("sqlite::memory:").await.unwrap();
    s.set_secret("openrouter", "k").await.unwrap();
    s
}

#[tokio::test]
async fn registry_hides_keyless_provider() {
    let s = crate::store::Store::open("sqlite::memory:").await.unwrap();
    let reg = ProviderRegistry::load(&s, "http://127.0.0.1:1").await.unwrap();
    let err = reg.engine(&ModelRef::parse("openrouter:x")).err().unwrap();
    assert!(matches!(err, ProviderError::Missing(m) if m.contains("OpenRouter key")));
    assert!(reg.remote_models(false).await.is_empty());
}

#[tokio::test]
async fn registry_isolates_failures() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).respond_with(ResponseTemplate::new(500)).mount(&server).await;
    let reg = ProviderRegistry::load(&keyed_store().await, &server.uri()).await.unwrap();
    let lists = reg.remote_models(false).await;
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].provider, "openrouter");
    assert!(lists[0].models.is_empty());
    assert!(lists[0].error.is_some());
}

#[tokio::test]
async fn remote_models_cached() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(models_fixture()))
        .mount(&server)
        .await;
    let reg = ProviderRegistry::load(&keyed_store().await, &server.uri()).await.unwrap();
    assert_eq!(reg.remote_models(false).await[0].models.len(), 4);
    reg.remote_models(false).await;
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    reg.remote_models(true).await;
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}
