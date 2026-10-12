//! The research agent's tools. Every tool returns text for the model; failures
//! are text too ("Error: …"), so the model can recover instead of the loop dying.
use agentix::raw::shared::FunctionDefinition;
use agentix::ToolDefinition;
use serde_json::json;

use crate::search::SearchProvider;
use crate::web;

pub use crate::web::FetchPolicy;

pub const WEB_SEARCH: &str = "web_search";
pub const FETCH_PAGE: &str = "fetch_page";
pub const SUBMIT_PROFILE: &str = "submit_profile";

const SEARCH_RESULTS: u8 = 5;
const PAGE_CHARS: usize = 5000;

fn def(name: &str, description: &str, parameters: serde_json::Value) -> ToolDefinition {
    ToolDefinition::function(FunctionDefinition {
        name: name.into(),
        description: Some(description.into()),
        parameters,
        strict: None,
    })
}

pub fn definitions() -> Vec<ToolDefinition> {
    let text = json!({"type": "string"});
    vec![
        def(
            WEB_SEARCH,
            "Search the web. Returns the top results with title, URL and an excerpt.",
            json!({"type": "object", "properties": {"query": text}, "required": ["query"]}),
        ),
        def(
            FETCH_PAGE,
            "Read a web page (http or https) as plain text, truncated.",
            json!({"type": "object", "properties": {"url": text}, "required": ["url"]}),
        ),
        def(
            SUBMIT_PROFILE,
            "Submit the final company profile. Call exactly once; it ends the research. Use \"\" for unknown fields.",
            json!({
                "type": "object",
                "properties": {
                    "canonical_name": text, "domain": text, "summary": text, "industry": text,
                    "size": {"type": "string", "description": "Headcount band, e.g. 51-200"},
                    "stage": {"type": "string", "description": "e.g. seed, series B, public"},
                    "headquarters": text, "remote_policy": text, "products": text,
                    "tech_stack": {"type": "array", "items": text},
                    "sources": {"type": "array", "items": {"type": "object",
                        "properties": {"url": text, "title": text}, "required": ["url"]}},
                },
                "required": ["canonical_name"],
            }),
        ),
    ]
}

pub async fn web_search(search: &dyn SearchProvider, query: &str) -> String {
    match search.search(query, SEARCH_RESULTS).await {
        Ok(hits) if hits.is_empty() => "No results.".into(),
        Ok(hits) => hits
            .iter()
            .map(|h| format!("{} — {}\n{}", h.title, h.url, h.snippet.trim()))
            .collect::<Vec<_>>()
            .join("\n\n"),
        Err(e) => format!("Error: search failed: {e}"),
    }
}

/// What the agent reads: the page text, or an "Error: …" line it can recover from.
#[cfg_attr(not(test), allow(dead_code))]
pub async fn fetch_page(http: &reqwest::Client, url: &str) -> String {
    fetch_page_with(http, url, FetchPolicy::PublicOnly).await
}

pub async fn fetch_page_with(_http: &reqwest::Client, url: &str, policy: FetchPolicy) -> String {
    match web::fetch_text(url, policy, PAGE_CHARS).await {
        Ok(text) => text,
        Err(e) => format!("Error: {e}"),
    }
}
