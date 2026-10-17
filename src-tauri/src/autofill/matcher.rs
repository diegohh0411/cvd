//! Deterministic field → fact matching (harness steps 1 and 3). Anything it is
//! not sure about goes to the user; the Fast Model takes over those later.

use super::facts::fold_accents;
use super::field::{FieldKind, FieldSpec};
use super::Fact;

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Fill { fact: &'static str, value: String },
    Resume,
    NeedsUser(String),
}

fn needs(reason: &str) -> Decision {
    Decision::NeedsUser(reason.into())
}

/// Lowercase, accent-free, punctuation as spaces, single-spaced.
pub fn normalize(s: &str) -> String {
    fold_accents(s)
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && *w != "required")
        .collect::<Vec<_>>()
        .join(" ")
}

const SENSITIVE: &[&str] = &[
    "gender", "race", "racial", "ethnic", "hispanic", "latino", "veteran", "disab", "sexual orientation",
    "transgender", "pronoun", "date of birth", "birth date", "religio", "marital",
];

/// Questions about someone else, where the user's own phone or name would be wrong.
const OTHER_PERSON: &[&str] = &["emergency", "reference", "referr", "referred", "recruiter", "manager", "spouse", "hear about"];

const EXACT: &[(&str, &[&str])] = &[
    ("first_name", &["first name", "given name", "legal first name", "first", "forename", "preferred first name"]),
    ("last_name", &["last name", "surname", "family name", "legal last name", "last"]),
    ("full_name", &["name", "full name", "legal name", "your name", "full legal name"]),
    ("email", &["email", "email address", "e mail", "e mail address"]),
    ("phone", &["phone", "phone number", "mobile", "mobile number", "mobile phone", "telephone", "cell phone", "phone mobile"]),
    ("city", &["city", "current city", "city of residence", "location city"]),
    ("region", &["state", "province", "state province", "region"]),
    ("country", &["country", "country of residence", "current country"]),
    ("location_full", &["location", "current location", "where are you located", "where are you based"]),
    ("linkedin", &["linkedin", "linkedin profile", "linkedin url", "linkedin profile url"]),
    ("github", &["github", "github url", "github profile"]),
    ("website", &["website", "personal website", "portfolio", "portfolio url", "website url", "blog", "portfolio website"]),
    ("current_company", &["current company", "current employer", "company", "current company name"]),
    ("current_title", &["current title", "current job title", "current role", "current position"]),
    ("years_experience", &["years of experience", "total years of experience"]),
    ("notice_period", &["notice period", "what is your notice period"]),
    ("earliest_start", &["start date", "earliest start date", "available start date", "when can you start"]),
];

/// Long yes/no questions, matched by substring. Order matters: sponsorship
/// questions often also say "authorized to work".
const CONTAINS: &[(&str, &[&str])] = &[
    ("needs_sponsorship", &["sponsor"]),
    (
        "work_authorization",
        &["authorized to work", "authorised to work", "legally authorized", "work authorization", "eligible to work", "right to work"],
    ),
    ("willing_to_relocate", &["relocat"]),
    ("earliest_start", &["earliest you", "when can you start", "when could you start", "available to start"]),
];

fn by_autocomplete(ac: &str) -> Option<&'static str> {
    Some(match ac.split_whitespace().last()? {
        "given-name" => "first_name",
        "family-name" => "last_name",
        "name" => "full_name",
        "email" => "email",
        "tel" | "tel-national" => "phone",
        "address-level2" => "city",
        "address-level1" => "region",
        "country-name" => "country",
        "organization" => "current_company",
        "organization-title" => "current_title",
        _ => return None,
    })
}

fn by_label(label: &str) -> Option<&'static str> {
    if let Some((key, _)) = EXACT.iter().find(|(_, pats)| pats.contains(&label)) {
        return Some(key);
    }
    CONTAINS.iter().find(|(_, pats)| pats.iter().any(|p| label.contains(p))).map(|(k, _)| *k)
}

/// `job_application[first_name]` → `first name`.
fn name_tail(name: &str) -> String {
    normalize(name.split(['[', ']', '.']).filter(|s| !s.is_empty()).last().unwrap_or(""))
}

fn is_yes_no(fact: &str) -> bool {
    matches!(fact, "needs_sponsorship" | "work_authorization" | "willing_to_relocate")
}

/// The option whose text is one of the fact's spellings, or for yes/no facts
/// the one that starts with that answer ("Yes, I am authorized").
fn match_option(options: &[String], fact: &Fact) -> Option<String> {
    let variants: Vec<String> = fact.variants.iter().map(|v| normalize(v)).collect();
    let norm: Vec<String> = options.iter().map(|o| normalize(o)).collect();
    if let Some(i) = norm.iter().position(|o| variants.contains(o)) {
        return Some(options[i].clone());
    }
    if is_yes_no(fact.key) {
        let word = variants.first()?;
        let hits: Vec<usize> = norm.iter().enumerate().filter(|(_, o)| o.split(' ').next() == Some(word)).map(|(i, _)| i).collect();
        if let [i] = hits[..] {
            return Some(options[i].clone());
        }
    }
    None
}

pub fn decide(field: &FieldSpec, facts: &[Fact]) -> Decision {
    let label = normalize(&field.label);
    let context = normalize(&format!("{} {} {}", field.label, field.help, field.section));
    let kind = field.kind();

    if kind == FieldKind::File {
        let hay = normalize(&format!("{} {} {}", field.label, field.section, field.name_attr));
        let resume = ["resume", "cv", "curriculum"].iter().any(|w| hay.split(' ').any(|t| t == *w));
        return if resume { Decision::Resume } else { needs("File upload") };
    }
    if SENSITIVE.iter().any(|w| context.contains(w)) {
        return needs("Personal question: cvd never answers these");
    }
    let salary = ["salary", "compensation", "pay expectation"].iter().any(|w| context.contains(w));
    if kind == FieldKind::Date {
        return needs("Pick a date");
    }
    if matches!(kind, FieldKind::Textarea | FieldKind::Checkbox | FieldKind::Other) && !salary {
        return needs("Needs your own answer");
    }
    if OTHER_PERSON.iter().any(|w| context.contains(w)) {
        return needs("Asks about someone else");
    }

    let fact_key = if salary {
        Some("salary_expectation")
    } else {
        by_autocomplete(&field.autocomplete)
            .or_else(|| by_label(&label))
            // Placeholder and name attribute only stand in for a missing label: a label
            // cvd doesn't recognise means the question is something else.
            .or_else(|| if label.is_empty() { by_label(&normalize(&field.placeholder)) } else { None })
            .or_else(|| if label.is_empty() { by_label(&name_tail(&field.name_attr)) } else { None })
            .or(match kind {
                FieldKind::Email => Some("email"),
                FieldKind::Tel => Some("phone"),
                _ => None,
            })
    };
    let Some(fact_key) = fact_key else {
        return needs("Not sure what this asks");
    };
    let Some(fact) = facts.iter().find(|f| f.key == fact_key) else {
        return needs(if salary { "Salary: set an expectation in Application defaults, or answer here" } else { "Not in your profile yet" });
    };

    let value = match kind {
        FieldKind::Select | FieldKind::Radio | FieldKind::Combobox if field.options.is_empty() => {
            return needs("Pick from the list");
        }
        FieldKind::Select | FieldKind::Radio | FieldKind::Combobox => match match_option(&field.options, fact) {
            Some(o) => o,
            None => return needs("None of the options matches your profile"),
        },
        FieldKind::Number => match fact.variants.iter().find(|v| v.parse::<f64>().is_ok()) {
            Some(v) => v.clone(),
            None => return needs("Expects a number"),
        },
        _ => fact.variants[0].clone(),
    };
    Decision::Fill { fact: fact.key, value }
}
