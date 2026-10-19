//! The research loop. Rounds are driven here rather than by `agentix::agent`
//! so we can cap rounds, trim old pages and report bad tool arguments to the
//! model (see the 2026-10-08 spike).
use std::sync::Arc;
use std::time::Duration;

use agentix::{Content, Message, Request, ToolCall, UserContent};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::prompt;
use super::tools::{self, FETCH_PAGE, SUBMIT_PROFILE, WEB_SEARCH};
use crate::providers::{ChatEndpoint, ProviderError};
use crate::search::SearchProvider;
use crate::store::types::{CompanyProfile, Source};

/// Unit tests read pages from a local mock server; the app only reads public hosts.
const FETCH_POLICY: tools::FetchPolicy = if cfg!(test) {
    tools::FetchPolicy::LocalFirstHopOnly
} else {
    tools::FetchPolicy::PublicOnly
};

/// Fetched pages kept verbatim in the history; older ones become a placeholder.
const KEEP_PAGES: usize = 3;
/// Time the final "submit now" turn gets once the budget is spent.
const FINAL_TURN: Duration = Duration::from_secs(60);
/// Characters of the JD the agent sees.
pub const JD_SNIPPET_CHARS: usize = 1500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub max_rounds: u32,
    pub wall_clock: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_rounds: 25,
            wall_clock: Duration::from_secs(240),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchInput {
    pub name: String,
    pub domain: Option<String>,
    pub jd_snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResearchOutput {
    pub canonical_name: String,
    pub domain: Option<String>,
    pub profile: CompanyProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResearchStep {
    Searching { query: String },
    Reading { domain: String },
    Round { n: u32, max: u32 },
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ResearchError {
    #[error("Company research ran out of time or steps.")]
    Budget,
    #[error("Company research was cancelled.")]
    Cancelled,
    #[error("Research model error: {0}")]
    Model(String),
    #[error(transparent)]
    Search(#[from] ProviderError),
    /// The result could not be saved (e.g. a concurrent merge); not the company's fault.
    #[error("Could not save company research: {0}")]
    Persist(String),
    /// Research can't run yet (missing key or model); not a failure of the company.
    #[error("{0}")]
    Unavailable(String),
}

#[derive(Deserialize)]
struct SubmitArgs {
    canonical_name: String,
    #[serde(default)]
    domain: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    industry: String,
    #[serde(default)]
    size: String,
    #[serde(default)]
    stage: String,
    #[serde(default)]
    headquarters: String,
    #[serde(default)]
    remote_policy: String,
    #[serde(default)]
    products: String,
    #[serde(default)]
    tech_stack: Vec<String>,
    #[serde(default)]
    sources: Vec<SubmitSource>,
}

#[derive(Deserialize)]
struct SubmitSource {
    url: String,
    #[serde(default)]
    title: String,
}

impl SubmitArgs {
    fn into_output(self) -> ResearchOutput {
        let domain = self.domain.trim().trim_start_matches("www.").to_lowercase();
        ResearchOutput {
            canonical_name: self.canonical_name.trim().to_string(),
            domain: (!domain.is_empty()).then_some(domain),
            profile: CompanyProfile {
                summary: self.summary,
                industry: self.industry,
                size: self.size,
                stage: self.stage,
                headquarters: self.headquarters,
                remote_policy: self.remote_policy,
                products: self.products,
                tech_stack: self.tech_stack,
                sources: self
                    .sources
                    .into_iter()
                    .map(|s| Source {
                        url: s.url,
                        title: s.title,
                    })
                    .collect(),
            },
        }
    }
}

pub async fn research(
    ep: &ChatEndpoint,
    search: Arc<dyn SearchProvider>,
    input: ResearchInput,
    cancel: CancellationToken,
    on_step: impl Fn(ResearchStep) + Send + Sync,
) -> Result<ResearchOutput, ResearchError> {
    research_with(ep, search, input, cancel, Budget::default(), on_step).await
}

/// What one tool call produced.
enum Outcome {
    Text(String),
    Submitted(Box<ResearchOutput>),
}

struct Loop<'a, F> {
    ep: &'a ChatEndpoint,
    search: Arc<dyn SearchProvider>,
    http: reqwest::Client,
    history: Vec<Message>,
    /// History indices of fetched-page tool results, oldest first, with their URLs.
    /// (Indices, not call ids: some servers reuse ids.)
    pages: Vec<(usize, String)>,
    /// URL of the page the current tool call fetched, recorded once its result is pushed.
    fetched: Option<String>,
    on_step: F,
}

impl<F: Fn(ResearchStep)> Loop<'_, F> {
    fn request(&self) -> Request {
        Request::openrouter(self.ep.api_key.clone().unwrap_or_default())
            .base_url(self.ep.base_url.clone())
            .model(self.ep.model.clone())
            .system_prompt(prompt::SYSTEM)
            .messages(self.history.clone())
            .tools(tools::definitions())
    }

    fn trim_pages(&mut self) {
        let old = self.pages.len().saturating_sub(KEEP_PAGES);
        for (i, url) in &self.pages[..old] {
            if let Some(Message::ToolResult { content, .. }) = self.history.get_mut(*i) {
                *content = vec![Content::Text {
                    text: format!("[page {url} read earlier — omitted]"),
                }];
            }
        }
    }

    async fn run_tool(&mut self, c: &ToolCall) -> Outcome {
        let args: serde_json::Value = match serde_json::from_str(&c.arguments) {
            Ok(v) => v,
            Err(e) => {
                return Outcome::Text(format!("Error: invalid JSON arguments for {}: {e}", c.name))
            }
        };
        let field = |k: &str| {
            args.get(k)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };
        match c.name.as_str() {
            WEB_SEARCH => {
                let query = field("query");
                (self.on_step)(ResearchStep::Searching { query: query.clone() });
                Outcome::Text(tools::web_search(&*self.search, &query).await)
            }
            FETCH_PAGE => {
                let url = field("url");
                let domain = reqwest::Url::parse(&url)
                    .ok()
                    .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string()))
                    .unwrap_or_default();
                (self.on_step)(ResearchStep::Reading { domain });
                self.fetched = Some(url.clone());
                Outcome::Text(tools::fetch_page_with(&self.http, &url, FETCH_POLICY).await)
            }
            SUBMIT_PROFILE => match serde_json::from_value::<SubmitArgs>(args) {
                Ok(a) if !a.canonical_name.trim().is_empty() => Outcome::Submitted(Box::new(a.into_output())),
                Ok(_) => Outcome::Text("Error: submit_profile needs a non-empty canonical_name.".into()),
                Err(e) => Outcome::Text(format!("Error: invalid submit_profile arguments: {e}")),
            },
            other => Outcome::Text(format!(
                "Error: unknown tool {other}. Available: {WEB_SEARCH}, {FETCH_PAGE}, {SUBMIT_PROFILE}."
            )),
        }
    }

    /// One model turn plus its tool calls. `Some` once a profile is submitted.
    async fn turn(
        &mut self,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> Result<Option<ResearchOutput>, ResearchError> {
        self.trim_pages();
        let req = self.request();
        let resp = tokio::select! {
            _ = cancel.cancelled() => return Err(ResearchError::Cancelled),
            _ = tokio::time::sleep_until(deadline) => return Err(ResearchError::Budget),
            r = req.complete(&self.http) => r.map_err(|e| ResearchError::Model(e.to_string()))?,
        };
        let calls = resp.tool_calls.clone();
        self.history.push(Message::Assistant {
            content: resp.content,
            reasoning: resp.reasoning,
            tool_calls: resp.tool_calls,
            provider_data: resp.provider_data,
        });
        if calls.is_empty() {
            self.history.push(Message::User(vec![UserContent::Text {
                text: prompt::NUDGE.into(),
            }]));
            return Ok(None);
        }
        for c in &calls {
            if cancel.is_cancelled() {
                return Err(ResearchError::Cancelled);
            }
            let text = match self.run_tool(c).await {
                Outcome::Submitted(out) => return Ok(Some(*out)),
                Outcome::Text(t) => t,
            };
            self.history.push(Message::ToolResult {
                call_id: c.id.clone(),
                content: vec![Content::Text { text }],
            });
            if let Some(url) = self.fetched.take() {
                self.pages.push((self.history.len() - 1, url));
            }
        }
        Ok(None)
    }
}

pub async fn research_with(
    ep: &ChatEndpoint,
    search: Arc<dyn SearchProvider>,
    input: ResearchInput,
    cancel: CancellationToken,
    budget: Budget,
    on_step: impl Fn(ResearchStep) + Send + Sync,
) -> Result<ResearchOutput, ResearchError> {
    if cancel.is_cancelled() {
        return Err(ResearchError::Cancelled);
    }
    let mut l = Loop {
        ep,
        search,
        http: reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("reqwest client"),
        history: vec![Message::User(vec![UserContent::Text {
            text: prompt::user_message(&input),
        }])],
        pages: Vec::new(),
        fetched: None,
        on_step,
    };
    let deadline = Instant::now() + budget.wall_clock;
    for n in 1..=budget.max_rounds {
        (l.on_step)(ResearchStep::Round {
            n,
            max: budget.max_rounds,
        });
        match l.turn(deadline, &cancel).await {
            Ok(Some(out)) => return Ok(out),
            Ok(None) => {}
            Err(ResearchError::Budget) => break,
            Err(e) => return Err(e),
        }
        if Instant::now() >= deadline {
            break;
        }
    }
    // Out of rounds or time: one last turn asking for whatever it has.
    l.history.push(Message::User(vec![UserContent::Text {
        text: prompt::EXHAUSTED.into(),
    }]));
    match l.turn(Instant::now() + FINAL_TURN, &cancel).await {
        Ok(Some(out)) => Ok(out),
        Ok(None) | Err(ResearchError::Budget) => Err(ResearchError::Budget),
        Err(e) => Err(e),
    }
}
