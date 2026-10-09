use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

use super::agent::*;
use super::tools::fetch_page;
use crate::providers::{ChatEndpoint, ProviderError};
use crate::search::{SearchHit, SearchProvider};

struct FakeSearch;

#[async_trait]
impl SearchProvider for FakeSearch {
    fn id(&self) -> &'static str {
        "fake"
    }
    async fn search(&self, query: &str, _count: u8) -> Result<Vec<SearchHit>, ProviderError> {
        Ok(vec![SearchHit { title: "Acme".into(), url: "https://acme.com".into(), snippet: format!("about {query}") }])
    }
}

/// Answers chat requests from a script; the last entry repeats.
struct Script(Vec<Value>, AtomicUsize);

impl Respond for Script {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let i = self.1.fetch_add(1, Ordering::SeqCst).min(self.0.len() - 1);
        ResponseTemplate::new(200).set_body_json(&self.0[i])
    }
}

fn call(name: &str, args: &str) -> Value {
    json!({"choices": [{"index": 0, "finish_reason": "tool_calls", "message": {"role": "assistant", "content": null,
        "tool_calls": [{"id": format!("c-{name}"), "type": "function", "function": {"name": name, "arguments": args}}]}}]})
}

fn submit() -> Value {
    call("submit_profile", r#"{"canonical_name":"Acme Inc","domain":"acme.com","industry":"Fintech","tech_stack":["Rust"],"sources":[{"url":"https://acme.com","title":"Acme"}]}"#)
}

async fn llm(script: Vec<Value>) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(Script(script, AtomicUsize::new(0)))
        .mount(&server)
        .await;
    server
}

/// Serves an HTML page with nav noise around the article.
async fn pages() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(ResponseTemplate::new(200).insert_header("content-type", "text/html").set_body_string(
            "<html><body><nav>Home Jobs</nav><article><h1>About Acme</h1><p>Acme builds payment rails for small businesses across Latin America and employs two hundred people.</p></article></body></html>",
        ))
        .mount(&server)
        .await;
    server
}

fn ep(server: &MockServer) -> ChatEndpoint {
    ChatEndpoint { base_url: format!("{}/v1", server.uri()), api_key: None, model: "m".into() }
}

fn input() -> ResearchInput {
    ResearchInput { name: "Acme".into(), domain: None, jd_snippet: "Rust engineer at Acme".into() }
}

async fn bodies(server: &MockServer) -> Vec<Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/v1/chat/completions")
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

fn small() -> Budget {
    Budget { max_rounds: 4, wall_clock: Duration::from_secs(30) }
}

#[tokio::test]
async fn happy_path_search_fetch_submit() {
    let site = pages().await;
    let page = format!("{}/page", site.uri());
    let server = llm(vec![
        call("web_search", r#"{"query":"Acme company"}"#),
        call("fetch_page", &json!({"url": page}).to_string()),
        submit(),
    ])
    .await;
    let steps = Arc::new(Mutex::new(Vec::new()));
    let sink = steps.clone();
    let out = research_with(&ep(&server), Arc::new(FakeSearch), input(), CancellationToken::new(), small(), move |s| {
        sink.lock().unwrap().push(s)
    })
    .await
    .unwrap();
    assert_eq!(out.canonical_name, "Acme Inc");
    assert_eq!(out.domain.as_deref(), Some("acme.com"));
    assert_eq!(out.profile.industry, "Fintech");
    assert_eq!(out.profile.tech_stack, vec!["Rust".to_string()]);
    assert_eq!(out.profile.sources[0].url, "https://acme.com");
    let steps = steps.lock().unwrap().clone();
    let kinds: Vec<_> = steps.iter().filter(|s| !matches!(s, ResearchStep::Round { .. })).cloned().collect();
    assert_eq!(kinds, vec![ResearchStep::Searching { query: "Acme company".into() }, ResearchStep::Reading { domain: "127.0.0.1".into() }]);
    assert!(steps.contains(&ResearchStep::Round { n: 1, max: 4 }));
    // The fetched page reached the model as readable text.
    let b = bodies(&server).await;
    assert!(b[2].to_string().contains("payment rails"));
}

#[tokio::test]
async fn malformed_tool_args_are_returned_to_model() {
    let server = llm(vec![call("web_search", "{not json"), submit()]).await;
    research_with(&ep(&server), Arc::new(FakeSearch), input(), CancellationToken::new(), small(), |_| {}).await.unwrap();
    let b = bodies(&server).await;
    assert_eq!(b.len(), 2);
    let msgs = b[1]["messages"].as_array().unwrap();
    let tool = msgs.iter().find(|m| m["role"] == "tool").expect("tool message");
    assert!(tool.to_string().contains("invalid JSON arguments"), "{tool}");
}

#[tokio::test]
async fn unknown_tool_is_returned_to_model() {
    let server = llm(vec![call("launch_rockets", "{}"), submit()]).await;
    research_with(&ep(&server), Arc::new(FakeSearch), input(), CancellationToken::new(), small(), |_| {}).await.unwrap();
    let b = bodies(&server).await;
    assert!(b[1].to_string().contains("unknown tool launch_rockets"));
}

#[tokio::test]
async fn invalid_submit_is_returned_to_model() {
    let server = llm(vec![call("submit_profile", r#"{"industry":"x"}"#), submit()]).await;
    let out = research_with(&ep(&server), Arc::new(FakeSearch), input(), CancellationToken::new(), small(), |_| {}).await.unwrap();
    assert_eq!(out.canonical_name, "Acme Inc");
    assert!(bodies(&server).await[1].to_string().contains("canonical_name"));
}

#[tokio::test]
async fn budget_forces_final_submit() {
    let mut script = vec![call("web_search", r#"{"query":"q"}"#); 4];
    script.push(submit());
    let server = llm(script).await;
    let out = research_with(&ep(&server), Arc::new(FakeSearch), input(), CancellationToken::new(), small(), |_| {}).await.unwrap();
    assert_eq!(out.canonical_name, "Acme Inc");
    let b = bodies(&server).await;
    assert_eq!(b.len(), 5);
    assert!(b[4].to_string().contains("Budget exhausted. Call submit_profile now with what you have."));
}

#[tokio::test]
async fn default_budget_is_25_rounds_and_4_minutes() {
    let b = Budget::default();
    assert_eq!(b.max_rounds, 25);
    assert_eq!(b.wall_clock, Duration::from_secs(240));
}

#[tokio::test]
async fn budget_without_submit_is_error() {
    let server = llm(vec![call("web_search", r#"{"query":"q"}"#)]).await;
    let err = research_with(&ep(&server), Arc::new(FakeSearch), input(), CancellationToken::new(), small(), |_| {}).await.unwrap_err();
    assert!(matches!(err, ResearchError::Budget), "{err:?}");
}

#[tokio::test]
async fn cancel_stops_loop() {
    let server = llm(vec![call("web_search", r#"{"query":"q"}"#)]).await;
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = research_with(&ep(&server), Arc::new(FakeSearch), input(), cancel, small(), |_| {}).await.unwrap_err();
    assert!(matches!(err, ResearchError::Cancelled));
    assert!(bodies(&server).await.is_empty());
}

#[tokio::test]
async fn old_pages_are_trimmed() {
    let site = pages().await;
    let page = |n: u32| call("fetch_page", &json!({"url": format!("{}/page?n={n}", site.uri())}).to_string());
    let script = vec![page(1), page(2), page(3), page(4), page(5), submit()];
    let llm2 = llm(script).await;
    let budget = Budget { max_rounds: 10, wall_clock: Duration::from_secs(30) };
    research_with(&ep(&llm2), Arc::new(FakeSearch), input(), CancellationToken::new(), budget, |_| {}).await.unwrap();
    let last = bodies(&llm2).await.pop().unwrap().to_string();
    assert_eq!(last.matches("payment rails").count(), 3, "{last}");
    assert_eq!(last.matches("read earlier — omitted").count(), 2);
}

#[tokio::test]
async fn fetch_page_rejects_file_scheme() {
    let http = reqwest::Client::new();
    let out = fetch_page(&http, "file:///etc/passwd").await;
    assert!(out.starts_with("Error:"), "{out}");
}

/// Live smoke test against a running chat server and Exa:
/// `CVD_CHAT_BASE=http://127.0.0.1:PORT/v1 EXA_API_KEY=... cargo test --lib live_research -- --ignored --nocapture`.
#[tokio::test]
#[ignore]
async fn live_research() {
    let ep = ChatEndpoint { base_url: std::env::var("CVD_CHAT_BASE").unwrap(), api_key: std::env::var("CVD_CHAT_KEY").ok(), model: std::env::var("CVD_CHAT_MODEL").unwrap_or_default() };
    let search = Arc::new(crate::search::ExaSearch::new(crate::search::EXA_BASE.into(), std::env::var("EXA_API_KEY").unwrap()));
    let input = ResearchInput { name: "Nubank".into(), domain: None, jd_snippet: "Senior Backend Engineer (Clojure) at Nubank, São Paulo.".into() };
    let start = std::time::Instant::now();
    let out = research(&ep, search, input, CancellationToken::new(), |s| println!("step: {s:?}")).await;
    println!("{:?} in {:?}", out, start.elapsed());
    assert!(out.is_ok());
}
