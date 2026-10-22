use super::*;

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

fn ids(marks: &[Mark], skipped: &[String]) -> Vec<String> {
    options(marks, skipped)
        .into_iter()
        .map(|(a, _)| a.id())
        .collect()
}

fn choice(c: &str, p: f64) -> Answer {
    Answer::Choice {
        choice: c.into(),
        probabilities: BTreeMap::from([(c.to_string(), p)]),
    }
}

#[test]
fn submit_marks_are_never_offered() {
    let mut flagged = mark(3, MarkKind::Button, "Continue");
    flagged.submit = true;
    let marks = [
        mark(1, MarkKind::Button, "Apply for this job"),
        mark(2, MarkKind::Button, "Submit Application"),
        flagged,
        mark(4, MarkKind::Link, "Send application"),
        mark(5, MarkKind::Field, "Email"),
    ];
    let got = ids(&marks, &[]);
    assert!(got.contains(&"click:1".to_string()));
    assert!(got.contains(&"fill:5".to_string()));
    for n in [2, 3, 4] {
        assert!(!got.contains(&format!("click:{n}")));
    }
    let (state, _) = question(&Step {
        url: "https://x",
        title: "t",
        marks: &marks,
        facts: &[],
        history: &[],
        skipped: &[],
        image: "img",
    });
    assert!(!state.contains("Submit Application"));
}

#[test]
fn skipped_options_are_removed_and_explained() {
    let marks = [mark(1, MarkKind::Button, "Next")];
    let skipped = vec!["click:1".to_string()];
    assert!(!ids(&marks, &skipped).contains(&"click:1".to_string()));
    let (state, qs) = question(&Step {
        url: "https://x",
        title: "t",
        marks: &marks,
        facts: &["email"],
        history: &[],
        skipped: &skipped,
        image: "img",
    });
    assert!(state.contains("click:1: skipped, low confidence"));
    let Question::Choice {
        options, criteria, ..
    } = &qs[QUESTION_ID]
    else {
        panic!("choice question")
    };
    assert!(!options.contains(&"click:1".to_string()));
    assert_eq!(options.len(), criteria.len());
}

#[test]
fn state_carries_screenshot_placeholder_and_recent_history() {
    let history: Vec<String> = (0..8).map(|i| format!("a{i}")).collect();
    let (state, _) = question(&Step {
        url: "https://x",
        title: "t",
        marks: &[],
        facts: &[],
        history: &history,
        skipped: &[],
        image: "cvd-screen-1",
    });
    assert!(state.contains("cvd-screen-1"));
    assert!(!state.contains("\"a2\"") && state.contains("\"a3\"") && state.contains("\"a7\""));
}

#[test]
fn thresholds() {
    assert_eq!(
        outcome(Some(&choice("click:4", 0.6))),
        Outcome::Act(Action::Click(4))
    );
    assert_eq!(
        outcome(Some(&choice("click:4", 0.59))),
        Outcome::Skip("click:4".into())
    );
    assert_eq!(
        outcome(Some(&choice("click:4", 0.25))),
        Outcome::Skip("click:4".into())
    );
    assert!(matches!(
        outcome(Some(&choice("click:4", 0.24))),
        Outcome::Stop(_)
    ));
    assert!(matches!(outcome(None), Outcome::Stop(_)));
    assert!(matches!(
        outcome(Some(&choice("dance", 0.9))),
        Outcome::Stop(_)
    ));
}

#[test]
fn action_ids_round_trip() {
    for a in [
        Action::Click(7),
        Action::Fill(112),
        Action::ScrollDown,
        Action::ScrollUp,
        Action::Back,
        Action::Done,
        Action::AskUser,
    ] {
        assert_eq!(Action::parse(&a.id()), Some(a));
    }
    assert_eq!(Action::parse("click:x"), None);
}
