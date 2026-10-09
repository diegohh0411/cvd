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

/// Which hosts `fetch_page` may read. The agent's URLs come from the model
/// (and so from web pages), so production only reads public addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchPolicy {
    PublicOnly,
    /// Tests: the first URL may be a local test server; every redirect is still checked.
    #[cfg_attr(not(test), allow(dead_code))]
    LocalFirstHopOnly,
}

const MAX_REDIRECTS: usize = 5;
const REFUSED: &str = "Error: refusing to read a private or local address";

fn private_v4(ip: std::net::Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || (a == 100 && (64..128).contains(&b))
}

fn private_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => private_v4(v4),
        std::net::IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return private_v4(v4);
            }
            let first = v6.segments()[0];
            v6.is_loopback() || v6.is_unspecified() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
}

/// True when every address the URL's host resolves to is public.
async fn is_public(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else { return false };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return !private_ip(ip);
    }
    let port = url.port_or_known_default().unwrap_or(80);
    match tokio::net::lookup_host((host, port)).await {
        Ok(addrs) => {
            let addrs: Vec<_> = addrs.collect();
            !addrs.is_empty() && addrs.iter().all(|a| !private_ip(a.ip()))
        }
        Err(_) => false,
    }
}

fn no_redirect_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(PAGE_TIMEOUT)
            .build()
            .expect("reqwest client")
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub async fn fetch_page(http: &reqwest::Client, url: &str) -> String {
    fetch_page_with(http, url, FetchPolicy::PublicOnly).await
}

/// Reads a page as text, following redirects by hand so each hop is checked.
pub async fn fetch_page_with(_http: &reqwest::Client, url: &str, policy: FetchPolicy) -> String {
    let mut current = match reqwest::Url::parse(url) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => u,
        _ => return format!("Error: only http and https URLs can be read, got {url}"),
    };
    let http = no_redirect_client();
    let mut hop = 0;
    let resp = loop {
        let local_ok = policy == FetchPolicy::LocalFirstHopOnly && hop == 0;
        if !local_ok && !is_public(&current).await {
            return format!("{REFUSED}: {current}");
        }
        let r = match http.get(current.clone()).send().await {
            Ok(r) => r,
            Err(e) => return format!("Error: could not fetch {url}: {e}"),
        };
        if !r.status().is_redirection() {
            break r;
        }
        hop += 1;
        let next = r
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|l| l.to_str().ok())
            .and_then(|l| current.join(l).ok())
            .filter(|u| matches!(u.scheme(), "http" | "https"));
        match next {
            Some(n) if hop <= MAX_REDIRECTS => current = n,
            _ => return format!("Error: too many or invalid redirects for {url}"),
        }
    };
    if !resp.status().is_success() {
        return format!("Error: HTTP {} for {url}", resp.status());
    }
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
