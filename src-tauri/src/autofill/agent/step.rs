//! One agent step as a System One `choice` question, and what its answer
//! means. Pure: no window, no engine.
use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::json;

use crate::autofill::field::FieldSpec;
use crate::engine::{Answer, Question};

/// Run the chosen option at or above this probability.
pub const ACT_THRESHOLD: f64 = 0.6;
/// Below this the agent is lost: stop the run.
pub const STOP_THRESHOLD: f64 = 0.25;
/// Re-asks after skips on one screen before the run stops.
pub const MAX_REASKS: usize = 3;
pub const MAX_STEPS: usize = 40;
/// Actions the model sees from earlier steps.
const HISTORY: usize = 5;
/// Marks offered per step; more would crowd the question.
const MAX_MARKS: usize = 120;
pub const QUESTION_ID: &str = "next";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkKind {
    Button,
    Link,
    Field,
}

/// A numbered element the page drew a badge on.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Mark {
    pub n: u32,
    pub kind: MarkKind,
    #[serde(default)]
    pub label: String,
    /// The page classed it as a submit control.
    #[serde(default)]
    pub submit: bool,
    /// Fields only: the control, as the DOM autofill describes it.
    #[serde(default)]
    pub field: Option<FieldSpec>,
    /// Fields only: it already has a value.
    #[serde(default)]
    pub filled: bool,
}

const SUBMIT_WORDS: &[&str] = &[
    "submit",
    "send application",
    "send my application",
    "complete application",
    "finish application",
    "confirm application",
];

/// Guard A: the page's verdict, or submit wording in the label.
pub fn is_submit(m: &Mark) -> bool {
    let label = m.label.to_lowercase();
    m.submit || SUBMIT_WORDS.iter().any(|w| label.contains(w))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Click(u32),
    Fill(u32),
    ScrollDown,
    ScrollUp,
    Back,
    Done,
    AskUser,
}

impl Action {
    pub fn id(&self) -> String {
        match self {
            Action::Click(n) => format!("click:{n}"),
            Action::Fill(n) => format!("fill:{n}"),
            Action::ScrollDown => "scroll_down".into(),
            Action::ScrollUp => "scroll_up".into(),
            Action::Back => "back".into(),
            Action::Done => "done".into(),
            Action::AskUser => "ask_user".into(),
        }
    }

    pub fn parse(id: &str) -> Option<Action> {
        let num = |s: &str| s.parse().ok();
        Some(match id {
            "scroll_down" => Action::ScrollDown,
            "scroll_up" => Action::ScrollUp,
            "back" => Action::Back,
            "done" => Action::Done,
            "ask_user" => Action::AskUser,
            _ => match id.split_once(':')? {
                ("click", n) => Action::Click(num(n)?),
                ("fill", n) => Action::Fill(num(n)?),
                _ => return None,
            },
        })
    }
}

/// Everything the model is shown for one step.
pub struct Step<'a> {
    pub url: &'a str,
    pub title: &'a str,
    pub marks: &'a [Mark],
    /// Names of the profile details cvd can fill (never their values).
    pub facts: &'a [&'a str],
    /// Earlier actions, oldest first.
    pub history: &'a [String],
    /// Options skipped on this screen for low confidence.
    pub skipped: &'a [String],
    /// Placeholder the screenshot replaces in the state.
    pub image: &'a str,
}

/// The options open to the model, each with a description.
pub fn options(marks: &[Mark], skipped: &[String]) -> Vec<(Action, String)> {
    let mut out: Vec<(Action, String)> = marks
        .iter()
        .filter(|m| !is_submit(m))
        .take(MAX_MARKS)
        .map(|m| match m.kind {
            MarkKind::Field => {
                let state = if m.filled { "has a value" } else { "empty" };
                (
                    Action::Fill(m.n),
                    format!("Fill field [{}] \"{}\" ({state}) from the profile", m.n, m.label),
                )
            }
            MarkKind::Button => (
                Action::Click(m.n),
                format!("Click button [{}] \"{}\"", m.n, m.label),
            ),
            MarkKind::Link => (
                Action::Click(m.n),
                format!("Click link [{}] \"{}\"", m.n, m.label),
            ),
        })
        .collect();
    out.extend([
        (Action::ScrollDown, "Scroll down to see more of the page".into()),
        (Action::ScrollUp, "Scroll up".into()),
        (Action::Back, "Go back to the previous page".into()),
        (
            Action::Done,
            "Every field cvd can fill is filled and only submitting is left".into(),
        ),
        (
            Action::AskUser,
            "Stuck: a login, CAPTCHA, or a question only the person can answer".into(),
        ),
    ]);
    out.retain(|(a, _)| !skipped.contains(&a.id()));
    out
}

/// The state text and the single question for a step.
pub fn question(step: &Step) -> (String, BTreeMap<String, Question>) {
    let marks: Vec<_> = step
        .marks
        .iter()
        .filter(|m| !is_submit(m))
        .take(MAX_MARKS)
        .map(|m| {
            json!({
                "n": m.n,
                "kind": match m.kind { MarkKind::Button => "button", MarkKind::Link => "link", MarkKind::Field => "field" },
                "label": m.label,
                "filled": m.filled,
            })
        })
        .collect();
    let recent = &step.history[step.history.len().saturating_sub(HISTORY)..];
    let skipped: Vec<String> = step
        .skipped
        .iter()
        .map(|s| format!("{s}: skipped, low confidence"))
        .collect();
    let state = json!({
        "screenshot": step.image,
        "page": { "url": step.url, "title": step.title },
        "numbered_elements": marks,
        "profile_details_available": step.facts,
        "previous_actions": recent,
        "skipped_on_this_screen": skipped,
    })
    .to_string();
    let opts = options(step.marks, step.skipped);
    let question = Question::Choice {
        instructions: json!(
            "You are applying to a job for the person in a browser. The screenshot shows the page; \
             numbered badges mark the elements you can use. Pick the single next action that moves \
             the application forward: open the application form, fill empty fields from the \
             profile, move to the next page of the form. Never submit; when only submitting is \
             left, pick done. Skipped actions were too uncertain; pick another."
        ),
        options: opts.iter().map(|(a, _)| a.id()).collect(),
        criteria: opts.into_iter().map(|(a, d)| (a.id(), d)).collect(),
    };
    (state, BTreeMap::from([(QUESTION_ID.to_string(), question)]))
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Act(Action),
    /// Too unsure to act; skip this option and ask again.
    Skip(String),
    /// Lost, or the answer made no sense.
    Stop(String),
}

/// Applies the two thresholds to the chosen option's probability.
pub fn outcome(answer: Option<&Answer>) -> Outcome {
    let Some(Answer::Choice {
        choice,
        probabilities,
    }) = answer
    else {
        return Outcome::Stop("The Fast Model gave no answer".into());
    };
    let p = probabilities.get(choice).copied().unwrap_or(0.0);
    if p < STOP_THRESHOLD {
        return Outcome::Stop("Unsure what to do next".into());
    }
    if p < ACT_THRESHOLD {
        return Outcome::Skip(choice.clone());
    }
    match Action::parse(choice) {
        Some(a) => Outcome::Act(a),
        None => Outcome::Stop(format!("Unknown action {choice}")),
    }
}

#[cfg(test)]
#[path = "step_tests.rs"]
mod tests;
