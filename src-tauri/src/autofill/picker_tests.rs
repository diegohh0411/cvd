use std::collections::{BTreeMap, HashMap};

use super::field::{FieldKind, FieldSpec, FillAction};
use super::picker::{candidates, ACCEPT_P};
use super::window::{plan, Session};
use super::Fact;
use crate::engine::{Answer, FakeDecisionEngine, Question};

fn facts() -> Vec<Fact> {
    let f = |key: &'static str, v: &[&str]| Fact {
        key,
        variants: v.iter().map(|s| s.to_string()).collect(),
    };
    vec![
        f("first_name", &["Diego"]),
        f("last_name", &["Hernández", "Hernandez"]),
        f("full_name", &["Diego Hernández", "Diego Hernandez"]),
        f("email", &["diego@example.com"]),
        f("phone", &["+52 81 1234 5678", "+528112345678"]),
        f("city", &["Monterrey"]),
        f("country", &["México", "Mexico"]),
        f("linkedin", &["https://linkedin.com/in/diego"]),
        f("github", &["https://github.com/diego"]),
        f("current_company", &["Acme"]),
        f("current_title", &["Engineer"]),
        f("years_experience", &["4"]),
        f("needs_sponsorship", &["No"]),
        f("willing_to_relocate", &["Yes"]),
    ]
}

fn field(key: &str, kind: FieldKind, label: &str, options: &[&str]) -> FieldSpec {
    FieldSpec {
        key: key.into(),
        kind: Some(kind),
        label: label.into(),
        options: options.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

/// A fake model: per field label, the category (round 1) and the option text
/// (round 2, substring) to pick, and how sure it is. Unknown labels answer `none`.
fn oracle(wants: &[(&str, &str, &str, f64)]) -> FakeDecisionEngine {
    let wants: HashMap<String, (String, String, f64)> = wants
        .iter()
        .map(|(l, c, w, p)| (l.to_string(), (c.to_string(), w.to_string(), *p)))
        .collect();
    FakeDecisionEngine::new(move |_state, _name, q| {
        let Question::Choice {
            instructions,
            options,
            ..
        } = q
        else {
            panic!("only choice questions")
        };
        let label = instructions["field"]["label"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let round1 = instructions["question"]
            .as_str()
            .unwrap_or("")
            .starts_with("What kind");
        let texts: BTreeMap<String, String> = instructions["options"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
            .collect();
        let (choice, p) = match wants.get(&label) {
            Some((cat, want, p)) => {
                let want = if round1 { cat } else { want };
                let hit = texts
                    .iter()
                    .find(|(_, t)| t.contains(want.as_str()))
                    .map(|(k, _)| k.clone());
                (hit.unwrap_or_else(|| "none".into()), *p)
            }
            None => ("none".to_string(), 0.9),
        };
        let probabilities = options
            .iter()
            .map(|o| (o.clone(), if *o == choice { p } else { 0.0 }))
            .collect();
        Answer::Choice {
            choice,
            probabilities,
        }
    })
}

#[test]
fn text_candidates_respect_kind_and_exclusions() {
    let f = facts();
    let c = candidates(&field("a", FieldKind::Url, "Profile", &[]), &f, &[]);
    assert_eq!(
        c.iter().map(|c| c.fact).collect::<Vec<_>>(),
        ["linkedin", "github"]
    );
    let c = candidates(&field("a", FieldKind::Number, "Experience", &[]), &f, &[]);
    assert_eq!(
        c.iter().map(|c| c.value.as_str()).collect::<Vec<_>>(),
        ["4"]
    );
    // Yes/no preferences are never offered as free text.
    let c = candidates(&field("a", FieldKind::Text, "Anything", &[]), &f, &[]);
    assert!(c.iter().all(|c| c.fact != "needs_sponsorship"));
    // A rejected spelling falls back to the next one.
    let c = candidates(
        &field("a", FieldKind::Text, "Apellido", &[]),
        &f,
        &["Hernández".into()],
    );
    assert!(c.iter().any(|c| c.value == "Hernandez"));
    assert!(!c.iter().any(|c| c.value == "Hernández"));
}

#[test]
fn option_candidates_come_from_the_page() {
    let f = facts();
    let c = candidates(
        &field(
            "a",
            FieldKind::Select,
            "Where do you live?",
            &["Canada", "Mexico (+52)", "Spain"],
        ),
        &f,
        &[],
    );
    assert_eq!(c.len(), 1);
    assert_eq!(
        (c[0].fact, c[0].value.as_str()),
        ("country", "Mexico (+52)")
    );
    // Nothing on the page matches the profile: no candidates, so no model call.
    assert!(candidates(
        &field("a", FieldKind::Select, "T-shirt size", &["S", "M", "L"]),
        &f,
        &[]
    )
    .is_empty());
}

#[test]
fn yes_no_options_offer_only_stored_preferences() {
    let f = facts();
    let c = candidates(
        &field(
            "a",
            FieldKind::Radio,
            "Open to moving to SF?",
            &["Yes", "No"],
        ),
        &f,
        &[],
    );
    let got: Vec<_> = c.iter().map(|c| (c.fact, c.value.as_str())).collect();
    assert_eq!(
        got,
        [("needs_sponsorship", "No"), ("willing_to_relocate", "Yes")]
    );
}

#[tokio::test]
async fn model_fills_what_rules_could_not() {
    let fields = vec![
        field("a", FieldKind::Text, "First name", &[]),
        field("b", FieldKind::Text, "Nombre de pila", &[]),
        field(
            "c",
            FieldKind::Radio,
            "Would you be willing to move to San Francisco?",
            &["Yes", "No"],
        ),
        field("d", FieldKind::Text, "Favourite colour", &[]),
    ];
    let engine = oracle(&[
        ("Nombre de pila", "identity", "first name: Diego", 0.95),
        (
            "Would you be willing to move to San Francisco?",
            "",
            "willing to relocate",
            0.9,
        ),
    ]);
    let mut s = Session::default();
    let a = plan(fields, &facts(), None, Some(&engine), &mut s).await;
    assert_eq!(
        a[0],
        FillAction::Set {
            key: "a".into(),
            value: "Diego".into(),
            fact: "first_name"
        }
    );
    assert_eq!(
        a[1],
        FillAction::Set {
            key: "b".into(),
            value: "Diego".into(),
            fact: "first_name"
        }
    );
    // "relocat" in "move"? No: the rule didn't fire, the model mapped it to the stored preference.
    assert_eq!(
        a[2],
        FillAction::Set {
            key: "c".into(),
            value: "Yes".into(),
            fact: "willing_to_relocate"
        }
    );
    assert!(matches!(a[3], FillAction::NeedsUser { .. }));
    assert_eq!(s.already.get("Nombre de pila"), Some(&"first_name"));
}

#[tokio::test]
async fn low_confidence_and_engine_errors_go_to_the_user() {
    let fields = vec![field("b", FieldKind::Text, "Nombre de pila", &[])];
    let engine = oracle(&[(
        "Nombre de pila",
        "identity",
        "first name: Diego",
        ACCEPT_P - 0.1,
    )]);
    let a = plan(
        fields.clone(),
        &facts(),
        None,
        Some(&engine),
        &mut Session::default(),
    )
    .await;
    assert!(
        matches!(&a[0], FillAction::NeedsUser { reason, .. } if reason.contains("Not sure enough"))
    );

    let engine = oracle(&[("Nombre de pila", "identity", "first name: Diego", 0.95)]);
    engine.fail_on_call(1);
    let a = plan(
        fields.clone(),
        &facts(),
        None,
        Some(&engine),
        &mut Session::default(),
    )
    .await;
    assert!(
        matches!(&a[0], FillAction::NeedsUser { reason, .. } if reason.contains("Fast Model unavailable"))
    );

    // No engine at all: rules only.
    let a = plan(fields, &facts(), None, None, &mut Session::default()).await;
    assert!(matches!(a[0], FillAction::NeedsUser { .. }));
}

#[tokio::test]
async fn category_round_narrows_long_lists() {
    // Plain text fields see every text fact (> 8), so the model picks a category first.
    let fields = vec![field("x", FieldKind::Text, "Empresa actual", &[])];
    let engine = oracle(&[("Empresa actual", "experience", "Acme", 0.9)]);
    let a = plan(
        fields.clone(),
        &facts(),
        None,
        Some(&engine),
        &mut Session::default(),
    )
    .await;
    assert_eq!(
        a[0],
        FillAction::Set {
            key: "x".into(),
            value: "Acme".into(),
            fact: "current_company"
        }
    );
    assert_eq!(engine.calls(), 2);

    // Category `none` stops before round 2.
    let engine = oracle(&[]);
    let a = plan(
        fields,
        &facts(),
        None,
        Some(&engine),
        &mut Session::default(),
    )
    .await;
    assert!(matches!(a[0], FillAction::NeedsUser { .. }));
    assert_eq!(engine.calls(), 1);
}

#[tokio::test]
async fn rejected_values_are_excluded_on_retry() {
    let f = vec![field("a", FieldKind::Text, "Last name", &[])];
    let mut s = Session::default();
    s.excluded.insert("a".into(), vec!["Hernández".into()]);
    // The rule's value was rejected, so the model gets the remaining spellings.
    let engine = oracle(&[("Last name", "identity", "last name: Hernandez", 0.9)]);
    let a = plan(f, &facts(), None, Some(&engine), &mut s).await;
    assert_eq!(
        a[0],
        FillAction::Set {
            key: "a".into(),
            value: "Hernandez".into(),
            fact: "last_name"
        }
    );
}
