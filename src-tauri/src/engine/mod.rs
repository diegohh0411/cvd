mod fake;
mod http;
mod types;
mod wire;

use async_trait::async_trait;
use std::collections::BTreeMap;

#[allow(unused_imports)]
pub use fake::FakeDecisionEngine;
#[allow(unused_imports)]
pub use http::HttpSystemOneEngine;
pub use types::{Answer, EngineError, Question};
#[allow(unused_imports)]
pub use wire::{parse_wire_response, wire_request};

pub const MAX_QUESTIONS: usize = 128;

#[async_trait]
pub trait DecisionEngine: Send + Sync {
    /// The most questions this engine accepts per request, in `1..=MAX_QUESTIONS`.
    fn max_questions(&self) -> usize {
        MAX_QUESTIONS
    }

    async fn decide(
        &self,
        state: &str,
        questions: &BTreeMap<String, Question>,
    ) -> Result<BTreeMap<String, Answer>, EngineError>;
}

/// Enforces the engine's question cap before calling it.
pub async fn decide_checked(
    engine: &dyn DecisionEngine,
    state: &str,
    questions: &BTreeMap<String, Question>,
) -> Result<BTreeMap<String, Answer>, EngineError> {
    let max = engine.max_questions();
    if questions.len() > max {
        return Err(EngineError::TooManyQuestions { max });
    }
    engine.decide(state, questions).await
}

#[cfg(test)]
mod tests;
