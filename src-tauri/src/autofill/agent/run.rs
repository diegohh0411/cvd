//! The agent loop: look at the page, pick one action, do it, repeat. The page
//! and screen sit behind [`Page`] so the loop runs on fakes in tests.
use async_trait::async_trait;
use serde::Deserialize;

use super::step::{self, Action, Mark, MarkKind, Outcome, Step, MAX_REASKS, MAX_STEPS};
use crate::engine::{DecisionEngine, EngineError, Image};

const SCREEN_PLACEHOLDER: &str = "cvd-screen";

/// What the marks request returns.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Screen {
    pub url: String,
    pub title: String,
    pub marks: Vec<Mark>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Acted {
    Ok,
    /// Guard B fired: the click would have submitted the form.
    SubmitBlocked,
    /// The element is gone.
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Filled {
    Filled,
    /// cvd has no value for it; the page marks it for the user.
    NeedsUser,
}

#[async_trait]
pub trait Page: Send + Sync {
    async fn lock(&self, on: bool);
    async fn screen(&self) -> Result<Screen, String>;
    /// Base64 PNG of the page as it looks now.
    async fn screenshot(&self) -> Result<String, String>;
    async fn act(&self, action: &Action, mark: Option<&Mark>) -> Acted;
    async fn fill(&self, mark: &Mark) -> Filled;
    /// Marks a field the agent skipped as needing the user.
    async fn needs_user(&self, mark: &Mark, reason: &str);
    /// The user pressed Stop, or the window closed.
    fn stopped(&self) -> bool;
    /// Debug builds: records one step for manual evaluation.
    fn dump(&self, _step: usize, _state: &str, _screenshot: &str, _answer: &str) {}
}

#[derive(Debug, Clone, PartialEq)]
pub enum End {
    /// Only submitting is left (the model said done, or guard B fired).
    ReadyToSubmit,
    AskUser,
    /// A stop-threshold answer, too many skips, or the step limit.
    Unsure(String),
    UserStopped,
    /// The model can't read images; fall back to the DOM path.
    NoVision,
    /// No screenshot; fall back to the DOM path.
    NoCapture(String),
    Failed(String),
}

impl End {
    /// The line shown in the page when the run ends.
    pub fn notice(&self) -> String {
        match self {
            End::ReadyToSubmit => "Ready for you to review and submit.".into(),
            End::AskUser => "cvd needs you here. Take over, then submit yourself.".into(),
            End::Unsure(why) => format!("cvd stopped: {why}. Take over from here."),
            End::UserStopped => "Stopped. The page is yours.".into(),
            End::NoVision => "This Fast Model can't read images, so cvd fills fields only.".into(),
            End::NoCapture(why) => format!("{why} Filling fields only."),
            End::Failed(why) => format!("cvd stopped: {why}"),
        }
    }

    /// The DOM-only path should take over the page.
    pub fn falls_back(&self) -> bool {
        matches!(self, End::NoVision | End::NoCapture(_))
    }
}

/// Runs the agent until it ends, then unlocks the page.
pub async fn run(page: &dyn Page, engine: &dyn DecisionEngine, facts: &[&str]) -> End {
    page.lock(true).await;
    let end = steps(page, engine, facts).await;
    page.lock(false).await;
    end
}

async fn steps(page: &dyn Page, engine: &dyn DecisionEngine, facts: &[&str]) -> End {
    let mut history: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut reasks = 0;
    for n in 0..MAX_STEPS {
        if page.stopped() {
            return End::UserStopped;
        }
        page.lock(true).await; // a new page after navigation starts unlocked
        let screen = match page.screen().await {
            Ok(s) => s,
            Err(e) => return End::Failed(e),
        };
        let shot = match page.screenshot().await {
            Ok(s) => s,
            Err(e) => return End::NoCapture(e),
        };
        let (state, questions) = step::question(&Step {
            url: &screen.url,
            title: &screen.title,
            marks: &screen.marks,
            facts,
            history: &history,
            skipped: &skipped,
            image: SCREEN_PLACEHOLDER,
        });
        let image = Image {
            placeholder: SCREEN_PLACEHOLDER.into(),
            png_base64: shot,
        };
        let answers = match engine
            .decide_with_images(&state, &questions, std::slice::from_ref(&image))
            .await
        {
            Ok(a) => a,
            Err(EngineError::ImagesUnsupported) => return End::NoVision,
            Err(e) => return End::Failed(e.to_string()),
        };
        if page.stopped() {
            return End::UserStopped;
        }
        let answer = answers.get(step::QUESTION_ID);
        page.dump(n, &state, &image.png_base64, &format!("{answer:?}"));
        let find = |n: u32| screen.marks.iter().find(|m| m.n == n);
        let action = match step::outcome(answer) {
            Outcome::Stop(why) => return End::Unsure(why),
            Outcome::Skip(id) => {
                if let Some(m) = Action::parse(&id).and_then(|a| match a {
                    Action::Fill(n) => find(n),
                    _ => None,
                }) {
                    page.needs_user(m, "cvd wasn't sure about this one").await;
                }
                if !note_skip(id, &mut skipped, &mut reasks) {
                    return End::Unsure("Unsure what to do next".into());
                }
                continue;
            }
            Outcome::Act(a) => a,
        };
        let id = action.id();
        let done = match &action {
            Action::Done => return End::ReadyToSubmit,
            Action::AskUser => return End::AskUser,
            Action::Fill(n) => match find(*n) {
                Some(m) if m.kind == MarkKind::Field && !m.filled => {
                    page.fill(m).await == Filled::Filled
                }
                _ => false,
            },
            Action::Click(n) => match find(*n) {
                // The model never sees submit marks; this catches a forged answer.
                Some(m) if step::is_submit(m) => return End::ReadyToSubmit,
                Some(m) => match page.act(&action, Some(m)).await {
                    Acted::Ok => true,
                    Acted::SubmitBlocked => return End::ReadyToSubmit,
                    Acted::Missing => false,
                },
                None => false,
            },
            _ => match page.act(&action, None).await {
                Acted::SubmitBlocked => return End::ReadyToSubmit,
                acted => acted == Acted::Ok,
            },
        };
        if done {
            history.push(id);
            skipped.clear();
            reasks = 0;
        } else if !note_skip(id, &mut skipped, &mut reasks) {
            return End::Unsure("Unsure what to do next".into());
        }
    }
    End::Unsure(format!("Reached the {MAX_STEPS}-step limit"))
}

/// Records a skip; `false` once the screen's re-ask budget is spent.
fn note_skip(id: String, skipped: &mut Vec<String>, reasks: &mut usize) -> bool {
    skipped.push(id);
    *reasks += 1;
    *reasks <= MAX_REASKS
}

#[cfg(test)]
#[path = "run_tests.rs"]
mod tests;
