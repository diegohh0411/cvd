//! The research agent's tools. Every tool returns text for the model; failures
//! are text too ("Error: …"), so the model can recover instead of the loop dying.
use std::time::Duration;

use agentix::raw::shared::FunctionDefinition;
use agentix::ToolDefinition;
use serde_json::json;

use crate::search::SearchProvider;

pub const WEB_SEARCH: &str = "web_search";
pub const FETCH_PAGE: &str = "fetch_page";
pub const SUBMIT_PROFILE: &str = "submit_profile";

const SEARCH_RESULTS: u8 = 5;
const PAGE_CHARS: usize = 5000;
const PAGE_BYTES: usize = 2 * 1024 * 1024;
const PAGE_TIMEOUT: Duration = Duration::from_secs(15);

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

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Readable article text (Readability), falling back to a plain HTML-to-text dump.
fn page_text(html: &str) -> String {
    let readable = dom_smoothie::Readability::new(html, None, None)
        .ok()
        .and_then(|mut r| r.parse().ok())
        .map(|a| collapse(&a.text_content))
        .filter(|t| !t.is_empty());
    readable.unwrap_or_else(|| collapse(&html2text::from_read(html.as_bytes(), 120).unwrap_or_default()))
}

pub async fn fetch_page(http: &reqwest::Client, url: &str) -> String {
    let parsed = match reqwest::Url::parse(url) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => u,
        _ => return format!("Error: only http and https URLs can be read, got {url}"),
    };
    let resp = match http.get(parsed).timeout(PAGE_TIMEOUT).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => return format!("Error: HTTP {} for {url}", r.status()),
        Err(e) => return format!("Error: could not fetch {url}: {e}"),
    };
    let mut body = Vec::new();
    let mut resp = resp;
    loop {
        match resp.chunk().await {
            Ok(Some(c)) if body.len() < PAGE_BYTES => body.extend_from_slice(&c),
            Ok(_) => break,
            Err(e) => return format!("Error: could not read {url}: {e}"),
        }
    }
    let text = page_text(&String::from_utf8_lossy(&body));
    match text.char_indices().nth(PAGE_CHARS) {
        Some((i, _)) => format!("{}…", &text[..i]),
        None if text.is_empty() => "Error: the page has no readable text.".into(),
        None => text,
    }
}
