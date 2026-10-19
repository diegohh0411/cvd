//! One-sentence summary of an application email, written by the chat model.

use serde_json::{json, Value};

use super::classify::{job_label, EmailInput, JobCtx};
use crate::commands::AppError;
use crate::providers::ChatEndpoint;

const SYSTEM_PROMPT: &str = "You summarize job application emails. Reply with exactly one plain-text sentence describing the application update. No markdown, no lists, no quotes.";

pub async fn summarize(
    client: &reqwest::Client,
    ep: &ChatEndpoint,
    email: &EmailInput,
    job: &JobCtx,
    previous: Option<&str>,
    next: &str,
) -> Result<String, AppError> {
    let user_prompt = format!(
        "Subject: {}\nFrom: {}\nJob: {}\nPrevious status: {}\nNew status: {}\n\nEmail body:\n{}",
        email.subject,
        email.sender,
        job_label(&job.company, &job.title),
        previous.unwrap_or("none"),
        next,
        email.body,
    );
    let body = json!({
        "model": ep.model,
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            { "role": "user", "content": user_prompt },
        ],
    });

    let mut request = client
        .post(format!(
            "{}/chat/completions",
            ep.base_url.trim_end_matches('/')
        ))
        .json(&body);
    if let Some(key) = &ep.api_key {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|e| AppError::ProviderUnavailable(e.to_string()))?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(AppError::ProviderAuth(format!("HTTP {status}")));
    }
    if status == reqwest::StatusCode::PAYMENT_REQUIRED {
        return Err(AppError::ProviderCredits(format!("HTTP {status}")));
    }
    if !status.is_success() {
        return Err(AppError::EngineUnavailable(format!("HTTP {status}")));
    }

    let value: Value = response
        .json()
        .await
        .map_err(|e| AppError::EngineUnavailable(e.to_string()))?;
    let text = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if text.is_empty() {
        return Err(AppError::EngineUnavailable(
            "empty summary from chat model".into(),
        ));
    }
    Ok(text.to_string())
}
