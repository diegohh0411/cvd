use super::facts::fold_accents;
use super::*;
use crate::store::types::{Entry, EntryKind, Profile};

const TODAY: (i32, u32) = (2026, 10);

fn profile() -> Profile {
    Profile {
        name: "Diego  Hernández Herrera".into(),
        headline: String::new(),
        email: "diego@example.com".into(),
        phone: "+52 81 1234 5678".into(),
        location: "Monterrey, México".into(),
        links: vec!["linkedin.com/in/diego".into(), "https://github.com/diego".into(), "diego.dev".into(), " ".into()],
        summary: String::new(),
    }
}

fn job(title: &str, start: &str, end: Option<&str>) -> Entry {
    Entry {
        id: 0,
        kind: EntryKind::Experience,
        title: title.into(),
        org: format!("{title} Co"),
        location: String::new(),
        start_date: start.into(),
        end_date: end.map(Into::into),
        description: String::new(),
        sort_hint: 0,
        archived: false,
        bullets: vec![],
        skills: vec![],
    }
}

fn get<'a>(facts: &'a [Fact], key: &str) -> Option<&'a [String]> {
    facts.iter().find(|f| f.key == key).map(|f| f.variants.as_slice())
}

#[test]
fn splits_name_keeping_both_surnames_with_ascii_variants() {
    let f = build_facts(&profile(), &[], &ApplyDefaults::default(), TODAY);
    assert_eq!(get(&f, "first_name").unwrap(), ["Diego"]);
    assert_eq!(get(&f, "last_name").unwrap(), ["Hernández Herrera", "Hernandez Herrera"]);
    assert_eq!(get(&f, "full_name").unwrap(), ["Diego Hernández Herrera", "Diego Hernandez Herrera"]);
}

#[test]
fn name_overrides_win_over_the_split() {
    let d = ApplyDefaults { last_name: "Hernández".into(), ..Default::default() };
    let f = build_facts(&profile(), &[], &d, TODAY);
    assert_eq!(get(&f, "first_name").unwrap(), ["Diego"]);
    assert_eq!(get(&f, "last_name").unwrap()[0], "Hernández");
    assert_eq!(get(&f, "full_name").unwrap()[0], "Diego Hernández");
}

#[test]
fn single_word_name_has_no_last_name() {
    let p = Profile { name: "Diego".into(), ..profile() };
    let f = build_facts(&p, &[], &ApplyDefaults::default(), TODAY);
    assert_eq!(get(&f, "first_name").unwrap(), ["Diego"]);
    assert!(get(&f, "last_name").is_none());
}

#[test]
fn phone_variants() {
    let f = build_facts(&profile(), &[], &ApplyDefaults::default(), TODAY);
    assert_eq!(get(&f, "phone").unwrap(), ["+52 81 1234 5678", "+528112345678", "528112345678"]);
    let p = Profile { phone: "81-1234".into(), ..profile() };
    let f = build_facts(&p, &[], &ApplyDefaults::default(), TODAY);
    assert_eq!(get(&f, "phone").unwrap(), ["81-1234", "811234"]);
}

#[test]
fn classifies_links_and_adds_scheme() {
    let f = build_facts(&profile(), &[], &ApplyDefaults::default(), TODAY);
    assert_eq!(get(&f, "linkedin").unwrap(), ["https://linkedin.com/in/diego", "linkedin.com/in/diego"]);
    assert_eq!(get(&f, "github").unwrap(), ["https://github.com/diego"]);
    assert_eq!(get(&f, "website").unwrap(), ["https://diego.dev", "diego.dev"]);
}

#[test]
fn location_prefers_structured_defaults_then_profile_text() {
    let f = build_facts(&profile(), &[], &ApplyDefaults::default(), TODAY);
    assert!(get(&f, "city").is_none());
    assert_eq!(get(&f, "location_full").unwrap(), ["Monterrey, México", "Monterrey, Mexico"]);

    let d = ApplyDefaults { city: "Monterrey".into(), region: "Nuevo León".into(), country: "Mexico".into(), ..Default::default() };
    let f = build_facts(&profile(), &[], &d, TODAY);
    assert_eq!(get(&f, "city").unwrap(), ["Monterrey"]);
    assert_eq!(get(&f, "region").unwrap(), ["Nuevo León", "Nuevo Leon"]);
    assert_eq!(get(&f, "location_full").unwrap()[0], "Monterrey, Nuevo León, Mexico");
}

#[test]
fn current_role_and_years_with_overlaps_counted_once() {
    let mut old = job("Intern", "2019-01", Some("2019-12"));
    let archived = Entry { archived: true, ..job("Hidden", "2010-01", None) };
    let overlapping = job("Contractor", "2023-06", Some("2024-03"));
    let current = job("Engineer", "2024-01", None);
    let mut project = job("Side", "2015-01", None);
    project.kind = EntryKind::Project;
    old.org = "Old Co".into();
    let f = build_facts(&profile(), &[old, archived, overlapping, current, project], &ApplyDefaults::default(), TODAY);
    assert_eq!(get(&f, "current_title").unwrap(), ["Engineer"]);
    assert_eq!(get(&f, "current_company").unwrap(), ["Engineer Co"]);
    // 2019: 12 months; 2023-06..2026-10: 41 months → 53 months → 4 years.
    assert_eq!(get(&f, "years_experience").unwrap(), ["4"]);
}

#[test]
fn no_experience_means_no_experience_facts() {
    let f = build_facts(&profile(), &[], &ApplyDefaults::default(), TODAY);
    assert!(get(&f, "current_title").is_none());
    assert!(get(&f, "years_experience").is_none());
}

#[test]
fn preferences_only_when_set() {
    let f = build_facts(&profile(), &[], &ApplyDefaults::default(), TODAY);
    for key in ["work_authorization", "needs_sponsorship", "willing_to_relocate", "notice_period", "salary_expectation"] {
        assert!(get(&f, key).is_none(), "{key}");
    }
    let d = ApplyDefaults {
        authorized_to_work: Some(true),
        needs_sponsorship: Some(false),
        notice_period: "2 weeks".into(),
        ..Default::default()
    };
    let f = build_facts(&profile(), &[], &d, TODAY);
    assert_eq!(get(&f, "work_authorization").unwrap(), ["Yes"]);
    assert_eq!(get(&f, "needs_sponsorship").unwrap(), ["No"]);
    assert_eq!(get(&f, "notice_period").unwrap(), ["2 weeks"]);
    assert!(get(&f, "willing_to_relocate").is_none());
}

#[test]
fn empty_profile_yields_no_facts() {
    assert!(build_facts(&Profile::default(), &[], &ApplyDefaults::default(), TODAY).is_empty());
}

#[test]
fn folds_accents() {
    assert_eq!(fold_accents("Hernández Núñez Çé"), "Hernandez Nunez Ce");
}

#[test]
fn defaults_read_tolerantly_and_trim() {
    assert_eq!(ApplyDefaults::from_setting(None), ApplyDefaults::default());
    assert_eq!(ApplyDefaults::from_setting(Some("not json")), ApplyDefaults::default());
    let d = ApplyDefaults::from_setting(Some(r#"{"city":" Monterrey ","needs_sponsorship":false}"#)).trimmed();
    assert_eq!(d.city, "Monterrey");
    assert_eq!(d.needs_sponsorship, Some(false));
    assert_eq!(d.authorized_to_work, None);
}

#[test]
fn location_drops_repeated_parts() {
    let d = ApplyDefaults { city: "Madrid".into(), region: "madrid".into(), country: "Spain".into(), ..Default::default() };
    let f = build_facts(&profile(), &[], &d, TODAY);
    assert_eq!(get(&f, "location_full").unwrap()[0], "Madrid, Spain");
}
