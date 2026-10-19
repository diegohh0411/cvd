//! Deterministic field → fact matching (harness steps 1 and 3). Anything it is
//! not sure about goes to the user; the Fast Model takes over those later.

use std::collections::BTreeMap;

use super::facts::fold_accents;
use super::field::{FieldKind, FieldSpec};
use super::Fact;

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Fill {
        fact: &'static str,
        value: String,
    },
    Resume,
    /// No rule matched, but nothing forbids filling it: the Fast Model may pick a candidate.
    Unsure(String),
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

/// Matched at word starts (see [`has`]), so "race" doesn't hit "embrace".
const SENSITIVE: &[&str] = &[
    "gender",
    "race",
    "racial",
    "ethnic",
    "hispanic",
    "latino",
    "veteran",
    "disab",
    "sexual orientation",
    "transgender",
    "pronouns",
    "date of birth",
    "birth date",
    "religio",
    "marital",
];

/// Agreements and consents are the user's to give, whatever the options say.
const LEGAL: &[&str] = &[
    "agree",
    "consent",
    "acknowledg",
    "arbitrat",
    "terms",
    "privacy",
    "policy",
    "certify",
    "attest",
    "signature",
];

/// Questions about someone else, where the user's own phone or name would be wrong.
const OTHER_PERSON: &[&str] = &[
    "emergency",
    "reference",
    "referr",
    "recruiter",
    "manager",
    "spouse",
    "hear about",
];

/// Whether `pattern` occurs in normalized `text` starting at a word boundary.
fn has(text: &str, pattern: &str) -> bool {
    format!(" {text}").contains(&format!(" {pattern}"))
}

const EXACT: &[(&str, &[&str])] = &[
    (
        "first_name",
        &[
            "first name",
            "given name",
            "legal first name",
            "first",
            "forename",
            "preferred first name",
        ],
    ),
    (
        "last_name",
        &[
            "last name",
            "surname",
            "family name",
            "legal last name",
            "last",
        ],
    ),
    (
        "full_name",
        &[
            "name",
            "full name",
            "legal name",
            "your name",
            "full legal name",
        ],
    ),
    (
        "email",
        &["email", "email address", "e mail", "e mail address"],
    ),
    (
        "phone",
        &[
            "phone",
            "phone number",
            "mobile",
            "mobile number",
            "mobile phone",
            "telephone",
            "cell phone",
            "phone mobile",
        ],
    ),
    (
        "city",
        &["city", "current city", "city of residence", "location city"],
    ),
    ("region", &["state", "province", "state province", "region"]),
    (
        "country",
        &["country", "country of residence", "current country"],
    ),
    (
        "location_full",
        &[
            "location",
            "current location",
            "where are you located",
            "where are you currently located",
            "where are you based",
            "where do you live",
            "where do you currently live",
        ],
    ),
    (
        "linkedin",
        &[
            "linkedin",
            "linkedin profile",
            "linkedin url",
            "linkedin profile url",
        ],
    ),
    ("github", &["github", "github url", "github profile"]),
    (
        "website",
        &[
            "website",
            "personal website",
            "portfolio",
            "portfolio url",
            "website url",
            "blog",
            "portfolio website",
        ],
    ),
    (
        "current_company",
        &[
            "current company",
            "current employer",
            "company",
            "current company name",
        ],
    ),
    (
        "current_title",
        &[
            "current title",
            "current job title",
            "current role",
            "current position",
        ],
    ),
    (
        "years_experience",
        &["years of experience", "total years of experience"],
    ),
    (
        "notice_period",
        &["notice period", "what is your notice period"],
    ),
    (
        "earliest_start",
        &[
            "start date",
            "earliest start date",
            "available start date",
            "when can you start",
        ],
    ),
];

/// Long yes/no questions, matched by substring. Order matters: sponsorship
/// questions often also say "authorized to work".
const CONTAINS: &[(&str, &[&str])] = &[
    ("needs_sponsorship", &["sponsor"]),
    (
        "work_authorization",
        &[
            "authorized to work",
            "authorised to work",
            "legally authorized",
            "work authorization",
            "eligible to work",
            "right to work",
        ],
    ),
    ("willing_to_relocate", &["relocat"]),
    (
        "earliest_start",
        &[
            "earliest you",
            "when can you start",
            "when could you start",
            "available to start",
        ],
    ),
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
    CONTAINS
        .iter()
        .find(|(_, pats)| pats.iter().any(|p| has(label, p)))
        .map(|(k, _)| *k)
}

/// `job_application[first_name]` → `first name`.
fn name_tail(name: &str) -> String {
    normalize(
        name.split(['[', ']', '.'])
            .filter(|s| !s.is_empty())
            .last()
            .unwrap_or(""),
    )
}

pub(crate) fn is_yes_no(fact: &str) -> bool {
    matches!(
        fact,
        "needs_sponsorship" | "work_authorization" | "willing_to_relocate"
    )
}

/// The option whose text is one of the fact's spellings, or for yes/no facts
/// the one that starts with that answer ("Yes, I am authorized").
pub(crate) fn match_option(options: &[String], fact: &Fact) -> Option<String> {
    let variants: Vec<String> = fact.variants.iter().map(|v| normalize(v)).collect();
    let norm: Vec<String> = options.iter().map(|o| normalize(o)).collect();
    if let Some(i) = norm.iter().position(|o| variants.contains(o)) {
        return Some(options[i].clone());
    }
    if is_yes_no(fact.key) {
        let word = variants.first()?;
        let hits: Vec<usize> = norm
            .iter()
            .enumerate()
            .filter(|(_, o)| o.split(' ').next() == Some(word))
            .map(|(i, _)| i)
            .collect();
        if let [i] = hits[..] {
            return Some(options[i].clone());
        }
    }
    None
}

fn is_salary(label: &str) -> bool {
    ["salary", "compensation", "pay expectation"]
        .iter()
        .any(|w| has(label, w))
}

/// Fields cvd never fills, whatever it knows: uploads, personal and legal
/// questions, free text, dates, questions about someone else.
fn hard_rule(field: &FieldSpec) -> Option<Decision> {
    let label = normalize(&field.label);
    let context = normalize(&format!("{} {} {}", field.label, field.help, field.section));
    let kind = field.kind();
    if kind == FieldKind::File {
        let hay = normalize(&format!(
            "{} {} {}",
            field.label, field.section, field.name_attr
        ));
        let resume = ["resume", "cv", "curriculum"]
            .iter()
            .any(|w| hay.split(' ').any(|t| t == *w));
        return Some(if resume {
            Decision::Resume
        } else {
            needs("File upload")
        });
    }
    if SENSITIVE.iter().any(|w| has(&context, w)) {
        return Some(needs("Personal question: cvd never answers these"));
    }
    if LEGAL.iter().any(|w| has(&context, w)) {
        return Some(needs("Agreement or consent: give it yourself"));
    }
    if kind == FieldKind::Date {
        return Some(needs("Pick a date"));
    }
    if matches!(
        kind,
        FieldKind::Textarea | FieldKind::Checkbox | FieldKind::Other
    ) && !is_salary(&label)
    {
        return Some(needs("Needs your own answer"));
    }
    if OTHER_PERSON.iter().any(|w| has(&context, w)) {
        return Some(needs("Asks about someone else"));
    }
    None
}

/// Whether cvd may learn from the user's answer to this field.
pub fn is_learnable(field: &FieldSpec) -> bool {
    !normalize(&field.label).is_empty()
        && !is_salary(&normalize(&field.label))
        && hard_rule(field).is_none()
}

pub fn decide(field: &FieldSpec, facts: &[Fact]) -> Decision {
    decide_with(field, facts, &BTreeMap::new())
}

/// `memory` maps normalized labels to fact keys the user taught cvd. It comes
/// after the hard rules, so a memory can never unlock a protected field.
pub fn decide_with(
    field: &FieldSpec,
    facts: &[Fact],
    memory: &BTreeMap<String, String>,
) -> Decision {
    if let Some(d) = hard_rule(field) {
        return d;
    }
    let label = normalize(&field.label);
    let kind = field.kind();
    let salary = is_salary(&label);
    let remembered = match memory.get(&label) {
        // The user said this label means that fact, which the profile no longer has.
        Some(k) if !facts.iter().any(|f| f.key == k) => return needs("Not in your profile yet"),
        Some(k) => facts.iter().find(|f| f.key == k).map(|f| f.key),
        None => None,
    };

    let fact_key = if salary {
        Some("salary_expectation")
    } else {
        remembered
            .or_else(|| by_autocomplete(&field.autocomplete))
            .or_else(|| by_label(&label))
            // Placeholder and name attribute only stand in for a missing label: a label
            // cvd doesn't recognise means the question is something else.
            .or_else(|| {
                if label.is_empty() {
                    by_label(&normalize(&field.placeholder))
                } else {
                    None
                }
            })
            .or_else(|| {
                if label.is_empty() {
                    by_label(&name_tail(&field.name_attr))
                } else {
                    None
                }
            })
            .or(match kind {
                FieldKind::Email => Some("email"),
                FieldKind::Tel => Some("phone"),
                _ => None,
            })
    };
    let Some(fact_key) = fact_key else {
        return Decision::Unsure("Not sure what this asks".into());
    };
    let Some(fact) = facts.iter().find(|f| f.key == fact_key) else {
        return needs(if salary {
            "Salary: set an expectation in Application defaults, or answer here"
        } else {
            "Not in your profile yet"
        });
    };

    let value = match kind {
        FieldKind::Select | FieldKind::Radio | FieldKind::Combobox if field.options.is_empty() => {
            return needs("Pick from the list");
        }
        FieldKind::Select | FieldKind::Radio | FieldKind::Combobox => {
            match match_option(&field.options, fact) {
                Some(o) => o,
                None => return Decision::Unsure("None of the options matches your profile".into()),
            }
        }
        FieldKind::Number => match fact.variants.iter().find(|v| v.parse::<f64>().is_ok()) {
            Some(v) => v.clone(),
            None => return needs("Expects a number"),
        },
        _ => fact.variants[0].clone(),
    };
    Decision::Fill {
        fact: fact.key,
        value,
    }
}
