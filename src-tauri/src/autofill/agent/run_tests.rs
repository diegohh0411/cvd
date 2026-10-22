use super::*;
use crate::engine::{Answer, FakeDecisionEngine};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn mark(n: u32, kind: MarkKind, label: &str) -> Mark {
    Mark {
        n,
        kind,
        label: label.into(),
        submit: false,
        field: None,
        filled: false,
        origin: None,
    }
}

#[derive(Default)]
struct FakePage {
    screens: Vec<Screen>,
    /// Index into `screens`; advanced by clicks.
    at: AtomicUsize,
    log: Mutex<Vec<String>>,
    stop_after_lock: AtomicBool,
    stop: AtomicBool,
    block_submit_on: Option<u32>,
    no_capture: bool,
    locked: AtomicBool,
}

impl FakePage {
    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
    fn push(&self, s: String) {
        self.log.lock().unwrap().push(s);
    }
}

#[async_trait]
impl Page for FakePage {
    async fn lock(&self, on: bool) {
        // Re-locking every step is expected; log only changes.
        if self.locked.swap(on, Ordering::SeqCst) != on {
            self.push(if on { "lock" } else { "unlock" }.into());
        }
        if on && self.stop_after_lock.load(Ordering::SeqCst) {
            self.stop.store(true, Ordering::SeqCst);
        }
    }
    async fn screen(&self) -> Result<Screen, String> {
        let i = self.at.load(Ordering::SeqCst).min(self.screens.len() - 1);
        Ok(self.screens[i].clone())
    }
    async fn screenshot(&self) -> Result<String, String> {
        if self.no_capture {
            Err("no permission".into())
        } else {
            Ok("PNG".into())
        }
    }
    async fn act(&self, action: &Action, _mark: Option<&Mark>) -> Acted {
        self.push(action.id());
        if let Action::Click(n) = action {
            if Some(*n) == self.block_submit_on {
                return Acted::SubmitBlocked;
            }
            self.at.fetch_add(1, Ordering::SeqCst);
        }
        Acted::Ok
    }
    async fn fill(&self, mark: &Mark) -> Filled {
        self.push(format!("fill:{}", mark.n));
        if mark.label == "Salary" {
            Filled::NeedsUser
        } else {
            Filled::Filled
        }
    }
    async fn needs_user(&self, mark: &Mark, _reason: &str) {
        self.push(format!("needs:{}", mark.n));
    }
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
}

/// Answers each call with the next `(choice, probability)`.
fn engine(script: Vec<(&'static str, f64)>) -> FakeDecisionEngine {
    let n = Arc::new(AtomicUsize::new(0));
    FakeDecisionEngine::new(move |_, _, _| {
        let (c, p) = script[n.fetch_add(1, Ordering::SeqCst).min(script.len() - 1)];
        Answer::Choice {
            choice: c.into(),
            probabilities: BTreeMap::from([(c.to_string(), p)]),
        }
    })
    .seeing()
}

fn screen(marks: Vec<Mark>) -> Screen {
    Screen {
        url: "https://jobs.example.com".into(),
        title: "Job".into(),
        marks,
    }
}

fn posting_then_form() -> Vec<Screen> {
    let mut submit = mark(9, MarkKind::Button, "Submit application");
    submit.submit = true;
    vec![
        screen(vec![mark(1, MarkKind::Button, "Apply now")]),
        screen(vec![
            mark(2, MarkKind::Field, "Email"),
            mark(3, MarkKind::Field, "Salary"),
            submit,
        ]),
    ]
}

#[tokio::test]
async fn applies_fills_and_stops_before_submit() {
    let page = FakePage {
        screens: posting_then_form(),
        ..Default::default()
    };
    let e = engine(vec![("click:1", 0.9), ("fill:2", 0.8), ("done", 0.7)]);
    assert_eq!(run(&page, &e, &["email"]).await, End::ReadyToSubmit);
    assert_eq!(page.log(), ["lock", "click:1", "fill:2", "unlock"]);
}

#[tokio::test]
async fn low_confidence_skips_then_stops() {
    let page = FakePage {
        screens: posting_then_form(),
        ..Default::default()
    };
    // Skip "click:1", then pick something hopeless.
    let e = engine(vec![("click:1", 0.4), ("scroll_down", 0.1)]);
    assert!(matches!(run(&page, &e, &[]).await, End::Unsure(_)));
    assert_eq!(page.log(), ["lock", "unlock"]);
}

#[tokio::test]
async fn skipped_fields_go_to_the_user() {
    let page = FakePage {
        screens: posting_then_form(),
        at: AtomicUsize::new(1),
        ..Default::default()
    };
    let e = engine(vec![("fill:3", 0.5), ("done", 0.9)]);
    assert_eq!(run(&page, &e, &[]).await, End::ReadyToSubmit);
    assert_eq!(page.log(), ["lock", "needs:3", "unlock"]);
}

#[tokio::test]
async fn reask_budget_is_bounded() {
    let page = FakePage {
        screens: posting_then_form(),
        ..Default::default()
    };
    let e = engine(vec![("scroll_down", 0.5)]);
    assert!(matches!(run(&page, &e, &[]).await, End::Unsure(_)));
    assert_eq!(e.calls(), MAX_REASKS + 1);
}

#[tokio::test]
async fn unfillable_fields_count_as_skips() {
    let page = FakePage {
        screens: posting_then_form(),
        at: AtomicUsize::new(1),
        ..Default::default()
    };
    let e = engine(vec![("fill:3", 0.9)]);
    assert!(matches!(run(&page, &e, &[]).await, End::Unsure(_)));
    assert_eq!(e.calls(), MAX_REASKS + 1);
}

#[tokio::test]
async fn step_limit_ends_the_run() {
    let page = FakePage {
        screens: posting_then_form(),
        ..Default::default()
    };
    let e = engine(vec![("scroll_down", 0.9)]);
    assert!(matches!(run(&page, &e, &[]).await, End::Unsure(_)));
    assert_eq!(e.calls(), MAX_STEPS);
    assert_eq!(page.log().last().unwrap(), "unlock");
}

#[tokio::test]
async fn submit_marks_are_never_clicked() {
    let page = FakePage {
        screens: posting_then_form(),
        at: AtomicUsize::new(1),
        ..Default::default()
    };
    let e = engine(vec![("click:9", 0.99)]);
    assert_eq!(run(&page, &e, &[]).await, End::ReadyToSubmit);
    assert_eq!(page.log(), ["lock", "unlock"]);
}

#[tokio::test]
async fn blocked_submit_ends_ready() {
    let page = FakePage {
        screens: posting_then_form(),
        block_submit_on: Some(1),
        ..Default::default()
    };
    let e = engine(vec![("click:1", 0.9)]);
    assert_eq!(run(&page, &e, &[]).await, End::ReadyToSubmit);
    assert_eq!(page.log().last().unwrap(), "unlock");
}

#[tokio::test]
async fn stop_button_ends_the_run() {
    let page = FakePage {
        screens: posting_then_form(),
        stop_after_lock: AtomicBool::new(true),
        ..Default::default()
    };
    let e = engine(vec![("click:1", 0.9)]);
    assert_eq!(run(&page, &e, &[]).await, End::UserStopped);
    assert_eq!(page.log(), ["lock", "unlock"]);
}

#[tokio::test]
async fn falls_back_without_capture_or_vision() {
    let page = FakePage {
        screens: posting_then_form(),
        no_capture: true,
        ..Default::default()
    };
    let end = run(&page, &engine(vec![("done", 1.0)]), &[]).await;
    assert!(matches!(end, End::NoCapture(_)) && end.falls_back());

    let page = FakePage {
        screens: posting_then_form(),
        ..Default::default()
    };
    let blind = FakeDecisionEngine::new(|_, _, _| Answer::Noul { p: 1.0 });
    assert_eq!(run(&page, &blind, &[]).await, End::NoVision);
    assert_eq!(page.log(), ["lock", "unlock"]);
}

#[tokio::test]
async fn engine_errors_end_the_run() {
    let page = FakePage {
        screens: posting_then_form(),
        ..Default::default()
    };
    let e = engine(vec![("click:1", 0.9)]);
    e.fail_on_call(1);
    assert!(matches!(run(&page, &e, &[]).await, End::Failed(_)));
    assert_eq!(page.log(), ["lock", "unlock"]);
}
