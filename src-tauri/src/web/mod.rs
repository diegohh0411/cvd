//! Reads public web pages as plain text, offline: no search or scraping API is called,
//! only the page itself. Shared by the company agent and the job posting import.
use std::time::Duration;

const MAX_REDIRECTS: usize = 5;
const PAGE_BYTES: usize = 2 * 1024 * 1024;
const PAGE_TIMEOUT: Duration = Duration::from_secs(15);

/// Which hosts may be read. URLs from the model (and so from web pages) and from the
/// user's pasted links are only read when public, so the app can't be aimed at the LAN.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchPolicy {
    PublicOnly,
    /// Tests: the first URL may be a local test server; every redirect is still checked.
    #[cfg_attr(not(test), allow(dead_code))]
    LocalFirstHopOnly,
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("only http and https URLs can be read, got {0}")]
    Scheme(String),
    #[error("refusing to read a private or local address: {0}")]
    Private(String),
    #[error("could not fetch {url}: {reason}")]
    Request { url: String, reason: String },
    #[error("too many or invalid redirects for {0}")]
    Redirect(String),
    #[error("HTTP {status} for {url}")]
    Status { status: reqwest::StatusCode, url: String },
    #[error("could not read {url}: {reason}")]
    Body { url: String, reason: String },
    #[error("the page has no readable text")]
    Empty,
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

/// Reads a page as readable text, following redirects by hand so each hop is checked.
/// Longer text is cut to `max_chars` and ends with "…".
pub async fn fetch_text(url: &str, policy: FetchPolicy, max_chars: usize) -> Result<String, FetchError> {
    let mut current = match reqwest::Url::parse(url) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => u,
        _ => return Err(FetchError::Scheme(url.into())),
    };
    let http = no_redirect_client();
    let mut hop = 0;
    let resp = loop {
        let local_ok = policy == FetchPolicy::LocalFirstHopOnly && hop == 0;
        if !local_ok && !is_public(&current).await {
            return Err(FetchError::Private(current.to_string()));
        }
        let r = http.get(current.clone()).send().await.map_err(|e| FetchError::Request {
            url: url.into(),
            reason: e.to_string(),
        })?;
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
            _ => return Err(FetchError::Redirect(url.into())),
        }
    };
    if !resp.status().is_success() {
        return Err(FetchError::Status { status: resp.status(), url: url.into() });
    }
    let mut body = Vec::new();
    let mut resp = resp;
    loop {
        match resp.chunk().await {
            Ok(Some(c)) if body.len() < PAGE_BYTES => body.extend_from_slice(&c),
            Ok(_) => break,
            Err(e) => return Err(FetchError::Body { url: url.into(), reason: e.to_string() }),
        }
    }
    let text = page_text(&String::from_utf8_lossy(&body));
    match text.char_indices().nth(max_chars) {
        Some((i, _)) => Ok(format!("{}…", &text[..i])),
        None if text.is_empty() => Err(FetchError::Empty),
        None => Ok(text),
    }
}

#[cfg(test)]
mod tests;
