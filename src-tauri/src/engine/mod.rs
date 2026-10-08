mod fake;
mod types;
mod wire;

use async_trait::async_trait;
use std::collections::BTreeMap;

#[allow(unused_imports)]
pub use fake::FakeDecisionEngine;
pub use types::{Answer, EngineError, Question};
#[allow(unused_imports)]
pub use wire::{parse_wire_response, wire_request};

pub const MAX_QUESTIONS: usize = 128;

#[async_trait]
pub trait DecisionEngine: Send + Sync {
    async fn decide(
        &self,
        state: &str,
        questions: &BTreeMap<String, Question>,
    ) -> Result<BTreeMap<String, Answer>, EngineError>;
}

/// Enforces the 128-question cap before calling the engine.
pub async fn decide_checked(
    engine: &dyn DecisionEngine,
    state: &str,
    questions: &BTreeMap<String, Question>,
) -> Result<BTreeMap<String, Answer>, EngineError> {
    if questions.len() > MAX_QUESTIONS {
        return Err(EngineError::TooManyQuestions);
    }
    engine.decide(state, questions).await
}

#[cfg(test)]
mod tests;
