//! User-owned OpenAI-compatible server. CVD never starts or installs it.
use super::{ChatEndpoint, Provider, ProviderError, RemoteModel};
use crate::engine::{DecisionEngine, HttpSystemOneEngine, Question};
use async_trait::async_trait;
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc, time::Duration};

pub const LOCAL_SERVER: &str = "localhost";
pub const LOCAL_URL_KEY: &str = "local_server_url";
pub const DEFAULT_LOCAL_URL: &str = "http://127.0.0.1:8080/v1";

pub fn normalize_url(raw: &str) -> Result<String, ProviderError> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| {
        ProviderError::Missing("Enter a URL such as http://127.0.0.1:8080/v1.".into())
    })?;
    let loopback = url.host_str().is_some_and(|h| {
        h == "localhost"
            || h.trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    });
    if !loopback
        || !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ProviderError::Missing("Local server URL must use HTTP or HTTPS on localhost, 127.0.0.1 or ::1, without credentials, query or fragment.".into()));
    }
    let base = url.as_str().trim_end_matches('/');
    Ok(if !url.path().trim_end_matches('/').ends_with("/v1") {
        format!("{base}/v1")
    } else {
        base.into()
    })
}

pub struct LocalProvider {
    base: String,
    key: Option<String>,
    http: reqwest::Client,
}
impl LocalProvider {
    pub fn new(base: String, key: Option<String>) -> Self {
        Self {
            base,
            key,
            http: reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("HTTP client"),
        }
    }

    /// A real decision request catches servers exposing chat only, or models without System One support.
    pub async fn test(&self, model: &str, role: &str) -> Result<(), ProviderError> {
        let models = self.list_models().await?;
        if !models.iter().any(|m| m.id == model) {
            return Err(ProviderError::Missing(format!(
                "Model {model} is not served at {}.",
                self.base
            )));
        }
        if role == "decision" {
            let questions = BTreeMap::from([(
                "test".into(),
                Question::Noul {
                    instructions: Value::String("Is this a connection test?".into()),
                },
            )]);
            let answers = self.engine(model)?.decide("Connection test.", &questions).await
                .map_err(|e| ProviderError::BadResponse(format!("Local server cannot run System One: {e}. It needs /v1/systemone and a compatible decision model (including logprobs/grammar support).")))?;
            if !matches!(answers.get("test"), Some(crate::engine::Answer::Noul { p }) if p.is_finite() && (0.0..=1.0).contains(p))
            {
                return Err(ProviderError::BadResponse(
                    "System One returned no valid test answer.".into(),
                ));
            }
        } else if role == "chat" {
            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": "Call connection_test."}],
                "max_tokens": 64,
                "tool_choice": {"type": "function", "function": {"name": "connection_test"}},
                "tools": [{
                    "type": "function",
                    "function": {
                        "name": "connection_test",
                        "description": "Connection test",
                        "parameters": {"type": "object", "properties": {}}
                    }
                }]
            });
            let mut req = self
                .http
                .post(format!("{}/chat/completions", self.base))
                .json(&body);
            if let Some(key) = &self.key {
                req = req.bearer_auth(key);
            }
            let resp = req.send().await.map_err(|e| self.transport(e))?;
            if !resp.status().is_success() {
                return Err(ProviderError::BadResponse(format!("Local chat/tool-calling test failed: HTTP {}. Start a tool-capable chat model with --jinja.",resp.status())));
            }
            let v: Value = resp
                .json()
                .await
                .map_err(|e| ProviderError::BadResponse(e.to_string()))?;
            if !v
                .pointer("/choices/0/message/tool_calls")
                .and_then(Value::as_array)
                .is_some_and(|calls| {
                    calls.iter().any(|c| {
                        c.pointer("/function/name").and_then(Value::as_str)
                            == Some("connection_test")
                    })
                })
            {
                return Err(ProviderError::BadResponse(
                    "Local server did not return a tool call. Use a tool-capable chat model and start llama-server with --jinja.".into(),
                ));
            }
        } else {
            return Err(ProviderError::Missing(
                "Choose decision or chat for the connection test.".into(),
            ));
        }
        Ok(())
    }

    fn transport(&self, e: reqwest::Error) -> ProviderError {
        let authority = reqwest::Url::parse(&self.base)
            .ok()
            .map(|u| u.authority().to_string())
            .unwrap_or_default();
        ProviderError::Network(if e.is_connect() {
            format!("Nothing listening on {authority}. Start your local server and check its URL.")
        } else if e.is_timeout() {
            format!("Local server at {authority} timed out.")
        } else {
            e.to_string()
        })
    }
}
#[async_trait]
impl Provider for LocalProvider {
    fn id(&self) -> &'static str {
        LOCAL_SERVER
    }
    async fn list_models(&self) -> Result<Vec<RemoteModel>, ProviderError> {
        let mut req = self.http.get(format!("{}/models", self.base));
        if let Some(key) = &self.key {
            req = req.bearer_auth(key);
        }
        let resp = req.send().await.map_err(|e| self.transport(e))?;
        if matches!(resp.status().as_u16(), 401 | 403) {
            return Err(ProviderError::KeyRejected("Local server"));
        }
        if !resp.status().is_success() {
            return Err(ProviderError::BadResponse(format!(
                "Local server models endpoint: HTTP {}",
                resp.status()
            )));
        }
        let v: Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        let data = v.get("data").and_then(Value::as_array).ok_or_else(|| {
            ProviderError::BadResponse("Local server /models must return a data array.".into())
        })?;
        data.iter()
            .map(|m| {
                let id = m
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        ProviderError::BadResponse(
                            "Local server returned a model without an id.".into(),
                        )
                    })?;
                Ok(RemoteModel {
                    provider: LOCAL_SERVER.into(),
                    id: id.into(),
                    display_name: id.into(),
                    context_length: None,
                    price_per_mtok_in: None,
                    vision: super::vision_from_architecture(m),
                })
            })
            .collect()
    }
    async fn list_chat_models(&self) -> Result<Vec<RemoteModel>, ProviderError> {
        self.list_models().await
    }
    fn engine(&self, model_id: &str) -> Result<Arc<dyn DecisionEngine>, ProviderError> {
        Ok(Arc::new(
            HttpSystemOneEngine::new(
                "Local server",
                self.base.trim_end_matches("/v1").into(),
                self.key.clone().unwrap_or_default(),
                model_id.into(),
            )
            .without_proxy(),
        ))
    }
    fn chat_endpoint(&self, model_id: &str) -> Result<ChatEndpoint, ProviderError> {
        Ok(ChatEndpoint {
            base_url: self.base.clone(),
            api_key: self.key.clone(),
            model: model_id.into(),
        })
    }
}
