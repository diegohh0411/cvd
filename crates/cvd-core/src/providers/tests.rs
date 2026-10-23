use super::*;
use wiremock::matchers::body_partial_json;

#[test]
fn subq_engine_caps_questions_at_16() {
    let engine = SubQProvider::new(SUBQ_BASE.into(), "k".into())
        .engine("subq-snap-preview")
        .unwrap();
    assert_eq!(engine.max_questions(), 16);
}
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn models_fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../tests/fixtures/openrouter_models.json")).unwrap()
}

#[test]
fn model_ref_parse() {
    let local = ModelRef::parse("clef-flash");
    assert_eq!(
        (local.provider.as_str(), local.id.as_str()),
        ("local", "clef-flash")
    );
    let remote = ModelRef::parse("openrouter:inception/mercury-decide:free");
    assert_eq!(
        (remote.provider.as_str(), remote.id.as_str()),
        ("openrouter", "inception/mercury-decide:free")
    );
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
    let models = OpenRouterProvider::new(server.uri(), "k".into())
        .list_models()
        .await
        .unwrap();
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
        .respond_with(ResponseTemplate::new(401).set_body_json(
            serde_json::json!({"error": {"message": "User not found.", "code": 401}}),
        ))
        .mount(&server)
        .await;
    let err = OpenRouterProvider::new(server.uri(), "bad".into())
        .validate_key()
        .await
        .unwrap_err();
    assert_eq!(err, ProviderError::Unauthorized);
}

async fn keyed_store() -> crate::store::TenantStore {
    let s = crate::store::TenantStore::open("sqlite::memory:")
        .await
        .unwrap();
    s.set_secret("openrouter", "k").await.unwrap();
    s
}

#[tokio::test]
async fn registry_hides_keyless_provider() {
    let s = crate::store::TenantStore::open("sqlite::memory:")
        .await
        .unwrap();
    let reg = ProviderRegistry::load(&s, "http://127.0.0.1:1", "http://127.0.0.1:1")
        .await
        .unwrap();
    let err = reg.engine(&ModelRef::parse("openrouter:x")).err().unwrap();
    assert!(matches!(err, ProviderError::Missing(m) if m.contains("OpenRouter key")));
    assert!(reg.remote_models(false).await.is_empty());
}

#[tokio::test]
async fn registry_isolates_failures() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let reg = ProviderRegistry::load(&keyed_store().await, &server.uri(), "http://127.0.0.1:1")
        .await
        .unwrap();
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
    let reg = ProviderRegistry::load(&keyed_store().await, &server.uri(), "http://127.0.0.1:1")
        .await
        .unwrap();
    assert_eq!(reg.remote_models(false).await[0].models.len(), 4);
    reg.remote_models(false).await;
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    reg.remote_models(true).await;
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

async fn keyed_subq_store() -> crate::store::TenantStore {
    let s = crate::store::TenantStore::open("sqlite::memory:")
        .await
        .unwrap();
    s.set_secret(SUBQ, "sq").await.unwrap();
    s
}

fn subq_models() -> serde_json::Value {
    serde_json::json!({
        "object": "list",
        "data": [
            {"id": "subq-preview", "name": "SubQ Preview", "object": "model", "created": 0, "owned_by": "subquadratic"},
            {"id": "subq-snap-preview", "name": "SubQ Snap Preview", "object": "model", "created": 0, "owned_by": "subquadratic"}
        ]
    })
}

#[tokio::test]
async fn subq_lists_only_decision_models() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer sq"))
        .respond_with(ResponseTemplate::new(200).set_body_json(subq_models()))
        .mount(&server)
        .await;
    let p = SubQProvider::new(server.uri(), "sq".into());
    let models = p.list_models().await.unwrap();
    assert_eq!(models.len(), 1);
    let snap = &models[0];
    assert_eq!(
        (
            snap.provider.as_str(),
            snap.id.as_str(),
            snap.display_name.as_str()
        ),
        ("subq", "subq-snap-preview", "SubQ Snap")
    );
    assert_eq!(snap.context_length, Some(393_216));
    assert_eq!(snap.price_per_mtok_in, Some(0.05));
    assert!(p.list_chat_models().await.unwrap().is_empty());
    assert!(p.chat_endpoint("subq-snap-preview").is_err());
}

#[tokio::test]
async fn subq_401_is_key_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(401).set_body_json(
            serde_json::json!({"detail": {"error_type": "auth", "message": "bad key"}}),
        ))
        .mount(&server)
        .await;
    let err = SubQProvider::new(server.uri(), "bad".into())
        .validate_key()
        .await
        .unwrap_err();
    assert_eq!(err, ProviderError::KeyRejected("SubQ"));
    assert_eq!(err.to_string(), "Check your SubQ key");
}

#[tokio::test]
async fn registry_routes_subq_decisions() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(subq_models()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer sq"))
        .and(body_partial_json(
            serde_json::json!({"model": "subq-snap-preview"}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                serde_json::from_str::<serde_json::Value>(include_str!(
                    "../../tests/fixtures/openrouter_systemone_response.json"
                ))
                .unwrap(),
            ),
        )
        .mount(&server)
        .await;
    let reg = ProviderRegistry::load(
        &keyed_subq_store().await,
        "http://127.0.0.1:1",
        &server.uri(),
    )
    .await
    .unwrap();
    let lists = reg.remote_models(false).await;
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].provider, "subq");
    assert_eq!(lists[0].models[0].id, "subq-snap-preview");
    let r = ModelRef::parse("subq:subq-snap-preview");
    assert_eq!(r.provider, "subq");
    assert!(reg.check(&r).is_ok());
    let engine = reg.engine(&r).unwrap();
    let qs = std::collections::BTreeMap::from([(
        "rust".to_string(),
        crate::engine::Question::Noul {
            instructions: "Mentions Rust?".into(),
        },
    )]);
    assert!(engine
        .decide("Rewrote the ledger in Rust.", &qs)
        .await
        .is_ok());
}

#[tokio::test]
async fn registry_hides_keyless_subq() {
    let s = crate::store::TenantStore::open("sqlite::memory:")
        .await
        .unwrap();
    let reg = ProviderRegistry::load(&s, "http://127.0.0.1:1", "http://127.0.0.1:1")
        .await
        .unwrap();
    let err = reg
        .check(&ModelRef::parse("subq:subq-snap-preview"))
        .unwrap_err();
    assert!(matches!(err, ProviderError::Missing(m) if m.contains("SubQ key")));
}

/// Live smoke test: `OPENROUTER_API_KEY=... cargo test --lib live_openrouter -- --ignored`.
#[tokio::test]
#[ignore]
async fn live_openrouter_smoke() {
    use crate::engine::{decide_checked, Question};
    let key = std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY");
    let p = OpenRouterProvider::new(OPENROUTER_BASE.into(), key);
    p.validate_key().await.unwrap();
    let ids: Vec<String> = p
        .list_models()
        .await
        .unwrap()
        .into_iter()
        .map(|m| m.id)
        .collect();
    let qs = std::collections::BTreeMap::from([(
        "rust".to_string(),
        Question::Noul {
            instructions: "Does this CV entry mention Rust?".into(),
        },
    )]);
    for id in [
        "typesafe/jev-1.13",
        "openai/gpt-6-luna-decisions",
        "inception/mercury-decide:free",
    ] {
        assert!(ids.iter().any(|m| m == id), "{id} not listed");
        let r = ModelRef::parse(&format!("openrouter:{id}"));
        let engine = p.engine(&r.id).unwrap();
        let answers = decide_checked(&*engine, "Rewrote the ledger in Rust.", &qs)
            .await
            .unwrap();
        println!("{id}: {:?}", answers["rust"]);
    }
}

#[test]
fn openrouter_chat_endpoint() {
    let ep = OpenRouterProvider::new("https://openrouter.ai/api".into(), "k".into())
        .chat_endpoint("deepseek/x")
        .unwrap();
    assert_eq!(
        ep,
        ChatEndpoint {
            base_url: "https://openrouter.ai/api/v1".into(),
            api_key: Some("k".into()),
            model: "deepseek/x".into()
        }
    );
}

#[tokio::test]
async fn openrouter_lists_tool_models_for_chat() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(query_param("supported_parameters", "tools"))
        .respond_with(ResponseTemplate::new(200).set_body_json(models_fixture()))
        .mount(&server)
        .await;
    let reg = ProviderRegistry::load(&keyed_store().await, &server.uri(), "http://127.0.0.1:1")
        .await
        .unwrap();
    assert_eq!(reg.remote_chat_models(false).await[0].models.len(), 4);
}

fn typesafe_models() -> serde_json::Value {
    serde_json::json!({"models": [
        {"name": "jev-latest", "description": "Stable", "release_date": "2026-09-15"},
        {"name": "jev-preview", "description": "Preview", "release_date": "2026-10-14"}
    ]})
}

#[tokio::test]
async fn typesafe_lists_jev_models() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer ts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(typesafe_models()))
        .mount(&server)
        .await;
    let p = TypeSafeProvider::new(server.uri(), "ts".into());
    let models = p.list_models().await.unwrap();
    let ids: Vec<_> = models
        .iter()
        .map(|m| (m.provider.as_str(), m.id.as_str(), m.display_name.as_str()))
        .collect();
    assert_eq!(
        ids,
        [
            ("typesafe", "jev-latest", "Jev (stable)"),
            ("typesafe", "jev-preview", "Jev (preview)")
        ]
    );
    assert_eq!(models[0].price_per_mtok_in, Some(0.042));
    assert!(p.list_chat_models().await.unwrap().is_empty());
    assert!(p.chat_endpoint("jev-latest").is_err());
}

#[tokio::test]
async fn typesafe_401_is_key_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let err = TypeSafeProvider::new(server.uri(), "bad".into())
        .validate_key()
        .await
        .unwrap_err();
    assert_eq!(err, ProviderError::KeyRejected("TypeSafe"));
    assert_eq!(err.to_string(), "Check your TypeSafe key");
}

#[tokio::test]
async fn registry_routes_typesafe_decisions() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/systemone"))
        .and(header("authorization", "Bearer ts"))
        .and(body_partial_json(
            serde_json::json!({"model": "jev-latest"}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                serde_json::from_str::<serde_json::Value>(include_str!(
                    "../../tests/fixtures/openrouter_systemone_response.json"
                ))
                .unwrap(),
            ),
        )
        .mount(&server)
        .await;
    let s = crate::store::TenantStore::open("sqlite::memory:")
        .await
        .unwrap();
    s.set_secret(TYPESAFE, "ts").await.unwrap();
    let reg = ProviderRegistry::load_with(
        &s,
        "http://127.0.0.1:1",
        "http://127.0.0.1:1",
        &server.uri(),
    )
    .await
    .unwrap();
    let r = ModelRef::parse("typesafe:jev-latest");
    assert!(reg.check(&r).is_ok());
    let qs = std::collections::BTreeMap::from([(
        "rust".to_string(),
        crate::engine::Question::Noul {
            instructions: "Mentions Rust?".into(),
        },
    )]);
    assert!(reg
        .engine(&r)
        .unwrap()
        .decide("Rewrote the ledger in Rust.", &qs)
        .await
        .is_ok());
}

#[tokio::test]
async fn registry_hides_keyless_typesafe() {
    let s = crate::store::TenantStore::open("sqlite::memory:")
        .await
        .unwrap();
    let reg = ProviderRegistry::load(&s, "http://127.0.0.1:1", "http://127.0.0.1:1")
        .await
        .unwrap();
    let err = reg
        .check(&ModelRef::parse("typesafe:jev-latest"))
        .unwrap_err();
    assert!(matches!(err, ProviderError::Missing(m) if m.contains("TypeSafe key")));
}

#[test]
fn connection_ids() {
    assert_eq!(kind_of("openrouter"), "openrouter");
    assert_eq!(kind_of("openrouter~2"), "openrouter");
    let rows = vec![Connection {
        id: "openrouter~2".into(),
        provider: "openrouter".into(),
        label: "Work".into(),
    }];
    assert_eq!(next_id("openrouter", &rows), "openrouter~3");
    assert_eq!(next_id("subq", &rows), "subq~2");
    let r = ModelRef::parse("openrouter~2:inception/mercury-decide:free");
    assert_eq!(
        (r.provider.as_str(), r.id.as_str()),
        ("openrouter~2", "inception/mercury-decide:free")
    );
    assert!(!r.is_local());
}

#[tokio::test]
async fn registry_builds_one_provider_per_connection() {
    let s = crate::store::TenantStore::open("sqlite::memory:")
        .await
        .unwrap();
    s.set_secret("openrouter", "k1").await.unwrap();
    s.set_secret("openrouter~2", "k2").await.unwrap();
    save_rows(
        &s,
        &[Connection {
            id: "openrouter~2".into(),
            provider: "openrouter".into(),
            label: "Work".into(),
        }],
    )
    .await
    .unwrap();
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(models_fixture()))
        .mount(&server)
        .await;
    let reg = ProviderRegistry::load(&s, &server.uri(), "http://127.0.0.1:1")
        .await
        .unwrap();
    let lists = reg.remote_models(false).await;
    let got: Vec<_> = lists
        .iter()
        .map(|l| (l.provider.as_str(), l.label.as_str()))
        .collect();
    assert_eq!(
        got,
        [("openrouter", "OpenRouter"), ("openrouter~2", "Work")]
    );
    assert!(reg.check(&ModelRef::parse("openrouter~2:x")).is_ok());
    assert!(reg.check(&ModelRef::parse("openrouter~3:x")).is_err());
}

#[tokio::test]
async fn connection_row_without_a_key_is_ignored() {
    let s = crate::store::TenantStore::open("sqlite::memory:")
        .await
        .unwrap();
    save_rows(
        &s,
        &[Connection {
            id: "subq~2".into(),
            provider: "subq".into(),
            label: "Ghost".into(),
        }],
    )
    .await
    .unwrap();
    let reg = ProviderRegistry::load(&s, "http://127.0.0.1:1", "http://127.0.0.1:1")
        .await
        .unwrap();
    assert!(reg.remote_models(false).await.is_empty());
}
