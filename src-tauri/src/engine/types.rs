use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Question {
    Noul { instructions: String },
    Choice { instructions: String, options: Vec<String> },
    Score { instructions: String, legend: BTreeMap<u8, String> },
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
    #[error("too many questions in one request (max 128)")]
    TooManyQuestions,
    #[error("Check your OpenRouter key")]
    Unauthorized,
    #[error("Out of OpenRouter credits")]
    InsufficientCredits,
    #[error("Rate limited by the provider; try again shortly")]
    RateLimited,
    #[error("Network error: {0}")]
    Network(String),
    #[error("Model no longer available: {0}")]
    ModelUnavailable(String),
}
