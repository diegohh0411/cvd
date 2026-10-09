//! Exa (`POST {base}/search`), returning a text excerpt per result.
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{SearchHit, SearchProvider};
use crate::providers::ProviderError;

pub const EXA: &str = "exa";
pub const EXA_BASE: &str = "https://api.exa.ai";
/// Characters of page text Exa returns per hit; enough for a useful snippet.
const SNIPPET_CHARS: u32 = 500;

pub struct ExaSearch {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

impl ExaSearch {
    pub fn new(base_url: String, api_key: String) -> Self {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).build().expect("reqwest client");
        Self { base_url: base_url.trim_end_matches('/').to_string(), api_key, http }
    }
}

fn to_hit(r: &Value) -> Option<SearchHit> {
    let s = |k: &str| r.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let url = s("url");
    (!url.is_empty()).then(|| SearchHit { title: s("title"), snippet: s("text"), url })
}

#[async_trait]
impl SearchProvider for ExaSearch {
    fn id(&self) -> &'static str {
        EXA
    }

    async fn search(&self, query: &str, count: u8) -> Result<Vec<SearchHit>, ProviderError> {
        let body = json!({
            "query": query,
            "type": "auto",
            "numResults": count,
            "contents": { "text": { "maxCharacters": SNIPPET_CHARS } },
        });
        let resp = self
            .http
            .post(format!("{}/search", self.base_url))
            .header("x-api-key", &self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::KeyRejected("Exa"));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::Network("Exa rate limit reached; try again later".into()));
        }
        if !status.is_success() {
            return Err(ProviderError::BadResponse(format!("HTTP {status}")));
        }
        let v: Value = resp.json().await.map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        let results = v
            .get("results")
            .and_then(Value::as_array)
            .ok_or_else(|| ProviderError::BadResponse("missing `results` array".into()))?;
        Ok(results.iter().filter_map(to_hit).collect())
    }
}
