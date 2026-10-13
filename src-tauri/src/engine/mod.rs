mod fake;
mod http;
mod sidecar;
mod types;
mod wire;

use async_trait::async_trait;
use std::collections::BTreeMap;

#[allow(unused_imports)]
pub use fake::FakeDecisionEngine;
#[allow(unused_imports)]
pub use http::HttpSystemOneEngine;
#[allow(unused_imports)]
pub use sidecar::{LlamaSidecar, TauriLauncher};
pub use types::{Answer, EngineError, Question};
#[allow(unused_imports)]
pub use wire::{parse_wire_response, wire_request};

pub const MAX_QUESTIONS: usize = 128;

#[async_trait]
pub trait DecisionEngine: Send + Sync {
    /// The provider's cap on questions per request. `None` means `MAX_QUESTIONS`.
    fn max_questions(&self) -> Option<usize> {
        None
    }

    async fn decide(
        &self,
        state: &str,
        questions: &BTreeMap<String, Question>,
    ) -> Result<BTreeMap<String, Answer>, EngineError>;
}

/// Enforces the engine's question cap (or `MAX_QUESTIONS`) before calling it.
pub async fn decide_checked(
    engine: &dyn DecisionEngine,
    state: &str,
    questions: &BTreeMap<String, Question>,
) -> Result<BTreeMap<String, Answer>, EngineError> {
    if questions.len() > engine.max_questions().unwrap_or(MAX_QUESTIONS) {
        return Err(EngineError::TooManyQuestions);
    }
    engine.decide(state, questions).await
}

#[cfg(test)]
mod tests;
