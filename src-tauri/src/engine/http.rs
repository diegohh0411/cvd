//! Generic client for remote `POST {base}/v1/systemone` endpoints (e.g. OpenRouter).
use std::collections::BTreeMap;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::StatusCode;
use serde_json::Value;

use super::types::{Answer, EngineError, Question};
use super::wire::{parse_wire_response, wire_request};
use super::DecisionEngine;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_RETRY_AFTER: Duration = Duration::from_secs(10);

pub struct HttpSystemOneEngine {
    /// Display name for user-facing errors, e.g. "OpenRouter".
    provider: &'static str,
    base_url: String,
    api_key: String,
    model: String,
    /// The provider's per-request question cap, if it has one.
    max_questions: Option<usize>,
    http: reqwest::Client,
}

impl HttpSystemOneEngine {
    pub fn new(provider: &'static str, base_url: String, api_key: String, model: String) -> Self {
        let http = reqwest::Client::builder().timeout(REQUEST_TIMEOUT).build().expect("reqwest client");
        Self { provider, base_url, api_key, model, max_questions: None, http }
    }

    pub fn with_max_questions(mut self, cap: Option<usize>) -> Self {
        self.max_questions = cap;
        self
    }

    async fn send(&self, body: &Value) -> Result<reqwest::Response, EngineError> {
        self.http
            .post(format!("{}/v1/systemone", self.base_url.trim_end_matches('/')))
            .bearer_auth(&self.api_key)
            .json(body)
            .send()
            .await
            .map_err(|e| EngineError::Network(e.to_string()))
    }
}

fn retry_after(resp: &reqwest::Response) -> Duration {
    resp.headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(Duration::from_secs(1), Duration::from_secs)
        .min(MAX_RETRY_AFTER)
}

/// OpenRouter errors live at `/error/message`; SubQ's at `/detail/message`.
fn error_message(v: &Value) -> String {
    v.pointer("/error/message")
        .or_else(|| v.pointer("/detail/message"))
        .and_then(Value::as_str)
        .unwrap_or("unknown error")
        .to_string()
}

#[async_trait]
impl DecisionEngine for HttpSystemOneEngine {
    fn max_questions(&self) -> Option<usize> {
        self.max_questions
    }

    async fn decide(
        &self,
        state: &str,
        questions: &BTreeMap<String, Question>,
    ) -> Result<BTreeMap<String, Answer>, EngineError> {
        let mut body = wire_request(state, questions);
        body["model"] = Value::String(self.model.clone());

        let mut resp = self.send(&body).await?;
        if resp.status() == StatusCode::TOO_MANY_REQUESTS {
            tokio::time::sleep(retry_after(&resp)).await;
            resp = self.send(&body).await?;
        }
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| EngineError::Network(e.to_string()))?;
        let json: Option<Value> = serde_json::from_slice(&bytes).ok();
        let msg = || json.as_ref().map(error_message).unwrap_or_else(|| String::from_utf8_lossy(&bytes).into());
        match status.as_u16() {
            401 | 403 => Err(EngineError::Unauthorized(self.provider)),
            402 => Err(EngineError::InsufficientCredits(self.provider)),
            429 => Err(EngineError::RateLimited),
            404 => Err(EngineError::ModelUnavailable(self.model.clone())),
            400 if msg().contains("does not exist") => Err(EngineError::ModelUnavailable(self.model.clone())),
            400 if msg().contains("token limit") => Err(EngineError::ContextOverflow),
            _ if !status.is_success() => Err(EngineError::BadResponse(format!("HTTP {status}: {}", msg()))),
            _ => match json {
                Some(v) => parse_wire_response(v),
                None => Err(EngineError::BadResponse(format!("HTTP {status}: {}", msg()))),
            },
        }
    }
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
