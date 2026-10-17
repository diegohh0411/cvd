use super::field::{FieldKind, FieldSpec};
use super::matcher::{decide, normalize, Decision};
use super::Fact;

fn facts() -> Vec<Fact> {
    let f = |key: &'static str, v: &[&str]| Fact { key, variants: v.iter().map(|s| s.to_string()).collect() };
    vec![
        f("first_name", &["Diego"]),
        f("last_name", &["Hernández Herrera", "Hernandez Herrera"]),
        f("full_name", &["Diego Hernández Herrera", "Diego Hernandez Herrera"]),
        f("email", &["diego@example.com"]),
        f("phone", &["+52 81 1234 5678", "+528112345678", "528112345678"]),
        f("city", &["Monterrey"]),
        f("country", &["México", "Mexico"]),
        f("linkedin", &["https://linkedin.com/in/diego", "linkedin.com/in/diego"]),
        f("years_experience", &["4"]),
        f("needs_sponsorship", &["No"]),
        f("work_authorization", &["Yes"]),
    ]
}

fn field(kind: FieldKind, label: &str) -> FieldSpec {
    FieldSpec { key: "k".into(), kind: Some(kind), label: label.into(), ..Default::default() }
}

fn fill(d: Decision) -> (&'static str, String) {
    match d {
        Decision::Fill { fact, value } => (fact, value),
        other => panic!("expected a fill, got {other:?}"),
    }
}

fn is_needs(d: &Decision) -> bool {
    matches!(d, Decision::NeedsUser(_))
}

#[test]
fn normalizes_labels() {
    assert_eq!(normalize("First Name *"), "first name");
    assert_eq!(normalize("LinkedIn Profile (required)"), "linkedin profile");
    assert_eq!(normalize("País de residencia"), "pais de residencia");
}

#[test]
fn fills_identity_and_contact_by_label() {
    let f = facts();
    assert_eq!(fill(decide(&field(FieldKind::Text, "First Name*"), &f)), ("first_name", "Diego".into()));
    assert_eq!(fill(decide(&field(FieldKind::Text, "Last name"), &f)).1, "Hernández Herrera");
    assert_eq!(fill(decide(&field(FieldKind::Text, "Full name"), &f)).0, "full_name");
    assert_eq!(fill(decide(&field(FieldKind::Text, "Email"), &f)).1, "diego@example.com");
    assert_eq!(fill(decide(&field(FieldKind::Text, "Phone"), &f)).1, "+52 81 1234 5678");
    assert_eq!(fill(decide(&field(FieldKind::Url, "LinkedIn Profile"), &f)).1, "https://linkedin.com/in/diego");
    assert_eq!(fill(decide(&field(FieldKind::Text, "Location (City)"), &f)).1, "Monterrey");
}

#[test]
fn autocomplete_and_input_type_win_without_a_known_label() {
    let f = facts();
    let mut x = field(FieldKind::Text, "Vorname");
    x.autocomplete = "section-a given-name".into();
    assert_eq!(fill(decide(&x, &f)).0, "first_name");
    assert_eq!(fill(decide(&field(FieldKind::Email, "Correo"), &f)).0, "email");
    assert_eq!(fill(decide(&field(FieldKind::Tel, "Teléfono"), &f)).0, "phone");
}

#[test]
fn placeholder_and_name_only_stand_in_for_a_missing_label() {
    let f = facts();
    let mut x = field(FieldKind::Text, "");
    x.name_attr = "job_application[first_name]".into();
    assert_eq!(fill(decide(&x, &f)).0, "first_name");
    let mut x = field(FieldKind::Text, "");
    x.placeholder = "Last name".into();
    assert_eq!(fill(decide(&x, &f)).0, "last_name");
    // An unknown label is a different question, whatever the name attribute says.
    let mut x = field(FieldKind::Text, "Preferred name");
    x.name_attr = "first_name".into();
    assert!(is_needs(&decide(&x, &f)));
}

#[test]
fn unknown_questions_go_to_the_user() {
    let f = facts();
    assert!(is_needs(&decide(&field(FieldKind::Text, "Favourite colour"), &f)));
    assert!(is_needs(&decide(&field(FieldKind::Text, "Company name of your reference"), &f)));
    assert!(is_needs(&decide(&field(FieldKind::Tel, "Emergency contact phone"), &f)));
    assert!(is_needs(&decide(&field(FieldKind::Text, "How did you hear about us?"), &f)));
}

#[test]
fn never_answers_sensitive_questions() {
    let f = facts();
    for label in ["Gender", "Are you Hispanic/Latino?", "Veteran Status", "Disability status", "Date of birth"] {
        let mut x = field(FieldKind::Select, label);
        x.options = vec!["Yes".into(), "No".into(), "Decline to self-identify".into()];
        assert!(is_needs(&decide(&x, &f)), "{label}");
    }
    let mut x = field(FieldKind::Radio, "Do you identify as transgender?");
    x.section = "Voluntary Self-Identification".into();
    assert!(is_needs(&decide(&x, &f)));
}

#[test]
fn free_text_checkboxes_and_dates_go_to_the_user() {
    let f = facts();
    assert!(is_needs(&decide(&field(FieldKind::Textarea, "Why do you want to work here?"), &f)));
    assert!(is_needs(&decide(&field(FieldKind::Checkbox, "I agree to the privacy policy"), &f)));
    assert!(is_needs(&decide(&field(FieldKind::Date, "Start date"), &f)));
}

#[test]
fn salary_needs_a_stored_expectation() {
    let mut f = facts();
    assert!(is_needs(&decide(&field(FieldKind::Text, "Desired salary"), &f)));
    f.push(Fact { key: "salary_expectation", variants: vec!["90k USD".into()] });
    assert_eq!(fill(decide(&field(FieldKind::Text, "Desired salary"), &f)).1, "90k USD");
}

#[test]
fn missing_fact_goes_to_the_user() {
    assert!(is_needs(&decide(&field(FieldKind::Url, "GitHub"), &facts())));
}

#[test]
fn yes_no_questions_pick_the_matching_option() {
    let f = facts();
    let mut x = field(FieldKind::Select, "Will you now or in the future require sponsorship for employment visa status?");
    x.options = vec!["Select...".into(), "Yes".into(), "No".into()];
    assert_eq!(fill(decide(&x, &f)), ("needs_sponsorship", "No".into()));

    let mut x = field(FieldKind::Radio, "Are you legally authorized to work in the country where this job is located?");
    x.options = vec!["Yes, I am authorized".into(), "No, I am not".into()];
    assert_eq!(fill(decide(&x, &f)).1, "Yes, I am authorized");
}

#[test]
fn ambiguous_or_missing_options_go_to_the_user() {
    let f = facts();
    let mut x = field(FieldKind::Select, "Do you require visa sponsorship?");
    x.options = vec!["No, not now".into(), "No, not ever".into(), "Yes".into()];
    assert!(is_needs(&decide(&x, &f)));
    // Comboboxes whose list isn't rendered yet have no options to choose from.
    assert!(is_needs(&decide(&field(FieldKind::Combobox, "Country"), &f)));
}

#[test]
fn selects_match_any_spelling() {
    let f = facts();
    let mut x = field(FieldKind::Select, "Country");
    x.options = vec!["Canada".into(), "Mexico".into(), "United States".into()];
    assert_eq!(fill(decide(&x, &f)).1, "Mexico");
    let mut x = field(FieldKind::Select, "Country");
    x.options = vec!["Canada".into(), "United States".into()];
    assert!(is_needs(&decide(&x, &f)));
}

#[test]
fn number_fields_take_numeric_values() {
    assert_eq!(fill(decide(&field(FieldKind::Number, "Years of experience"), &facts())).1, "4");
    assert!(is_needs(&decide(&field(FieldKind::Number, "Phone"), &[Fact { key: "phone", variants: vec!["n/a".into()] }])));
}

#[test]
fn resume_uploads_are_recognised_by_label_or_section() {
    let f = facts();
    let mut x = field(FieldKind::File, "Attach");
    x.section = "Resume/CV".into();
    assert_eq!(decide(&x, &f), Decision::Resume);
    assert_eq!(decide(&field(FieldKind::File, "Upload your CV"), &f), Decision::Resume);
    assert!(is_needs(&decide(&field(FieldKind::File, "Cover letter"), &f)));
}

#[test]
fn sanitizes_hostile_input() {
    let big = "é".repeat(400); // 800 bytes
    let x = FieldSpec { label: big, options: vec!["x".into(); 1000], ..Default::default() }.sanitized();
    assert!(x.label.len() <= 500);
    assert_eq!(x.options.len(), 300);
}

#[test]
fn long_start_date_questions() {
    let mut f = facts();
    let q = field(FieldKind::Text, "When is the earliest you would want to start working with us?");
    assert!(is_needs(&decide(&q, &f)));
    f.push(Fact { key: "earliest_start", variants: vec!["In two weeks".into()] });
    assert_eq!(fill(decide(&q, &f)).1, "In two weeks");
}
