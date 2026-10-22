use super::types::{Answer, EngineError, Image, Question};
use super::DecisionEngine;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

type AnswerFn = dyn Fn(&str, &str, &Question) -> Answer + Send + Sync;

pub struct FakeDecisionEngine {
    f: Box<AnswerFn>,
    calls: AtomicUsize,
    /// 1-based index of the call that fails; 0 means never.
    fail_on: AtomicUsize,
    /// Accepts image requests (answered like text ones) when set.
    sees: bool,
}

impl FakeDecisionEngine {
    pub fn new(f: impl Fn(&str, &str, &Question) -> Answer + Send + Sync + 'static) -> Self {
        Self {
            f: Box::new(f),
            calls: AtomicUsize::new(0),
            fail_on: AtomicUsize::new(0),
            sees: false,
        }
    }

    /// Makes the fake accept `decide_with_images`.
    pub fn seeing(mut self) -> Self {
        self.sees = true;
        self
    }

    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub fn fail_on_call(&self, n: usize) {
        self.fail_on.store(n, Ordering::SeqCst);
    }
}

#[async_trait]
impl DecisionEngine for FakeDecisionEngine {
    async fn decide(
        &self,
        state: &str,
        questions: &BTreeMap<String, Question>,
    ) -> Result<BTreeMap<String, Answer>, EngineError> {
        let mut out = BTreeMap::new();
        for (name, q) in questions {
            let n = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            if n == self.fail_on.load(Ordering::SeqCst) {
                return Err(EngineError::Unavailable(format!(
                    "fake failure on call {n}"
                )));
            }
            out.insert(name.clone(), (self.f)(state, name, q));
        }
        Ok(out)
    }

    async fn decide_with_images(
        &self,
        state: &str,
        questions: &BTreeMap<String, Question>,
        _images: &[Image],
    ) -> Result<BTreeMap<String, Answer>, EngineError> {
        if !self.sees {
            return Err(EngineError::ImagesUnsupported);
        }
        self.decide(state, questions).await
    }
}
