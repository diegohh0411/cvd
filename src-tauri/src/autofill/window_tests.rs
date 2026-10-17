use super::field::{FieldKind, FieldSpec, FillAction};
use super::window::{is_supported_url, plan, Resume};
use super::Fact;

#[test]
fn supported_hosts() {
    assert!(is_supported_url("https://job-boards.greenhouse.io/anthropic/jobs/1"));
    assert!(is_supported_url("https://boards.greenhouse.io/acme/jobs/1"));
    assert!(is_supported_url("https://jobs.lever.co/acme/123/apply"));
    assert!(is_supported_url("https://jobs.ashbyhq.com/acme/123/application"));
    assert!(!is_supported_url("http://boards.greenhouse.io/acme"));
    assert!(!is_supported_url("https://greenhouse.io.evil.com/x"));
    assert!(!is_supported_url("https://evilgreenhouse.io/x"));
    assert!(!is_supported_url("https://acme.com/careers?gh_jid=1"));
    assert!(!is_supported_url("not a url"));
}

fn field(key: &str, kind: FieldKind, label: &str) -> FieldSpec {
    FieldSpec { key: key.into(), kind: Some(kind), label: label.into(), ..Default::default() }
}

#[test]
fn plans_fills_files_and_handoffs() {
    let facts = vec![Fact { key: "first_name", variants: vec!["Diego".into()] }];
    let fields = vec![
        field("a", FieldKind::Text, "First name"),
        field("b", FieldKind::File, "Resume/CV"),
        field("c", FieldKind::Textarea, "Why us?"),
        field("", FieldKind::Text, "First name"),
    ];
    let resume = Resume { name: "cv.pdf".into(), b64: "AAA=".into() };
    let actions = plan(fields.clone(), &facts, Some(&resume));
    assert_eq!(actions.len(), 3, "fields without a key are dropped");
    assert_eq!(actions[0], FillAction::Set { key: "a".into(), value: "Diego".into(), fact: "first_name" });
    assert_eq!(actions[1], FillAction::File { key: "b".into(), name: "cv.pdf".into(), b64: "AAA=".into() });
    assert!(matches!(actions[2], FillAction::NeedsUser { .. }));

    let without = plan(fields, &facts, None);
    assert!(matches!(&without[1], FillAction::NeedsUser { reason, .. } if reason.contains("Export a CV")));
}

#[test]
fn actions_serialize_for_the_page() {
    let a = FillAction::Set { key: "a".into(), value: "Diego".into(), fact: "first_name" };
    assert_eq!(serde_json::to_value(&a).unwrap(), serde_json::json!({"action": "set", "key": "a", "value": "Diego", "fact": "first_name"}));
    let f: FieldSpec = serde_json::from_value(serde_json::json!({"key": "k", "kind": "weird", "label": "x", "extra": 1})).unwrap();
    assert_eq!(f.kind(), FieldKind::Other);
}
