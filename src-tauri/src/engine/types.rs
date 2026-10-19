use serde_json::Value;
use std::collections::BTreeMap;

/// `instructions` is a plain string or a JSON object that carries the data a
/// question refers to (e.g. `{"entry": {...}, "question": "... `entry` ..."}`).
#[derive(Debug, Clone, PartialEq)]
pub enum Question {
    Noul { instructions: Value },
    /// `criteria` optionally describes each option (key = option id), sent as the
    /// wire `criteria` map; options without an entry are sent undescribed.
    Choice { instructions: Value, options: Vec<String>, criteria: BTreeMap<String, String> },
    Score { instructions: Value, legend: BTreeMap<u8, String> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Noul { p: f64 },
    Choice { choice: String, probabilities: BTreeMap<String, f64> },
    Score { score: f64, probabilities: BTreeMap<String, f64> },
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum EngineError {
    #[error("decision engine unavailable: {0}")]
    Unavailable(String),
    #[error("input too large for the decision model's batch size")]
    ContextOverflow,
    #[error("bad response from decision engine: {0}")]
    BadResponse(String),
    #[error("too many questions in one request (limit {max})")]
    TooManyQuestions { max: usize },
    #[error("Check your {0} key")]
    Unauthorized(&'static str),
    #[error("Out of {0} credits")]
    InsufficientCredits(&'static str),
    #[error("Rate limited by the provider; try again shortly")]
    RateLimited,
    #[error("Network error: {0}")]
    Network(String),
    #[error("Model no longer available: {0}")]
    ModelUnavailable(String),
}
