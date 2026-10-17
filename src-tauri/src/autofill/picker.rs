//! Harness steps 4–6: build candidates from the user's own facts and let the
//! Fast Model pick one or NONE. The model never sees a value it could invent:
//! every option is a real fact spelling or one of the page's own options.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::field::{FieldKind, FieldSpec};
use super::matcher::{is_yes_no, match_option, normalize};
use super::Fact;
use crate::engine::{decide_checked, Answer, DecisionEngine, EngineError, Question};

/// Below this the field goes to the user. A wrong fill costs more than an empty one.
pub const ACCEPT_P: f64 = 0.6;
/// Longer lists are narrowed by a category round first.
const MAX_DIRECT: usize = 8;
const NONE: &str = "none";

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub fact: &'static str,
    /// What gets typed or selected on the page.
    pub value: String,
}

fn describe(fact: &str) -> &'static str {
    match fact {
        "first_name" => "first name",
        "last_name" => "last name",
        "full_name" => "full name",
        "email" => "email address",
        "phone" => "phone number",
        "city" => "city",
        "region" => "state or region",
        "country" => "country",
        "location_full" => "location",
        "linkedin" => "LinkedIn profile",
        "github" => "GitHub profile",
        "website" => "personal website",
        "current_title" => "current job title",
        "current_company" => "current employer",
        "years_experience" => "years of work experience",
        "work_authorization" => "authorized to work",
        "needs_sponsorship" => "needs visa sponsorship",
        "willing_to_relocate" => "willing to relocate",
        "notice_period" => "notice period",
        "earliest_start" => "earliest start date",
        "salary_expectation" => "salary expectation",
        _ => "other",
    }
}

fn category(fact: &str) -> &'static str {
    match fact {
        "first_name" | "last_name" | "full_name" => "identity",
        "email" | "phone" => "contact",
        "city" | "region" | "country" | "location_full" => "location",
        "linkedin" | "github" | "website" => "links",
        "current_title" | "current_company" | "years_experience" => "experience",
        _ => "availability",
    }
}

fn is_yes_no_options(options: &[String]) -> bool {
    let firsts: Vec<String> = options.iter().filter_map(|o| normalize(o).split(' ').next().map(str::to_string)).collect();
    firsts.iter().any(|w| w == "yes") && firsts.iter().any(|w| w == "no")
}

/// Loose match between a page option and a fact spelling, for long lists such
/// as countries ("Mexico" ~ "Mexico (+52)").
fn loosely_matches(option: &str, variant: &str) -> bool {
    let (o, v) = (normalize(option), normalize(variant));
    o == v || (v.len() >= 3 && o.contains(&v)) || (o.len() >= 3 && v.contains(&o))
}

/// Candidates for one field, minus values the page already rejected.
pub fn candidates(field: &FieldSpec, facts: &[Fact], excluded: &[String]) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    let mut add = |fact: &'static str, value: String| {
        if !excluded.contains(&value) && !out.iter().any(|c| c.value == value) {
            out.push(Candidate { fact, value });
        }
    };
    match field.kind() {
        FieldKind::Select | FieldKind::Radio | FieldKind::Combobox if !field.options.is_empty() => {
            if is_yes_no_options(&field.options) {
                // The model decides which stored preference the question is about;
                // the answer itself comes from the user's defaults.
                for f in facts.iter().filter(|f| is_yes_no(f.key)) {
                    if let Some(o) = match_option(&field.options, f) {
                        add(f.key, o);
                    }
                }
            } else {
                for f in facts.iter().filter(|f| !is_yes_no(f.key)) {
                    for o in &field.options {
                        if f.variants.iter().any(|v| loosely_matches(o, v)) {
                            add(f.key, o.clone());
                        }
                    }
                }
            }
        }
        FieldKind::Select | FieldKind::Radio | FieldKind::Combobox => {}
        kind => {
            for f in facts.iter().filter(|f| !is_yes_no(f.key)) {
                let ok = match kind {
                    FieldKind::Email => f.key == "email",
                    FieldKind::Tel => f.key == "phone",
                    FieldKind::Url => matches!(f.key, "linkedin" | "github" | "website"),
                    FieldKind::Number => f.variants.iter().any(|v| v.parse::<f64>().is_ok()),
                    FieldKind::Text => true,
                    _ => false,
                };
                if !ok {
                    continue;
                }
                // Accent-free spellings only matter when the page rejects the first one.
                let preferred = f.variants.iter().find(|v| !excluded.contains(v));
                if let Some(v) = preferred {
                    if kind != FieldKind::Number || v.parse::<f64>().is_ok() {
                        add(f.key, v.clone());
                    }
                }
            }
        }
    }
    out
}

/// One field waiting for the model.
pub struct Pending {
    pub field: FieldSpec,
    pub candidates: Vec<Candidate>,
}

pub enum Pick {
    Chosen(Candidate),
    NoneFits,
    LowConfidence,
}

fn field_json(f: &FieldSpec) -> Value {
    json!({
        "label": f.label,
        "kind": f.kind(),
        "required": f.required,
        "placeholder": f.placeholder,
        "help": f.help,
        "section": f.section,
        "options_on_page": if f.options.len() <= 20 { json!(f.options) } else { json!(format!("{} options", f.options.len())) },
    })
}

const STATE: &str = "You are filling in a job application form for the applicant, using only the applicant's own data. \
Pick an option only if it answers the field exactly. If unsure, or if the field asks for something else, pick `none`.";

fn choice(field: &FieldSpec, already: &BTreeMap<String, &'static str>, question: &str, options: &BTreeMap<String, String>) -> Question {
    Question::Choice {
        instructions: json!({
            "field": field_json(field),
            "already_filled": already,
            "options": options,
            "question": question,
        }),
        options: options.keys().cloned().collect(),
    }
}

async fn ask(engine: &dyn DecisionEngine, questions: BTreeMap<String, Question>) -> Result<BTreeMap<String, Answer>, EngineError> {
    let mut out = BTreeMap::new();
    let max = engine.max_questions().max(1);
    let all: Vec<(String, Question)> = questions.into_iter().collect();
    for chunk in all.chunks(max) {
        let batch: BTreeMap<String, Question> = chunk.iter().cloned().collect();
        out.extend(decide_checked(engine, STATE, &batch).await?);
    }
    Ok(out)
}

fn chosen(a: Option<&Answer>) -> Option<(&str, f64)> {
    match a? {
        Answer::Choice { choice, probabilities } => Some((choice.as_str(), probabilities.get(choice).copied().unwrap_or(0.0))),
        _ => None,
    }
}

/// Runs the category round (for long lists) and the pick round, both batched
/// across fields. Returns one result per pending field, in order.
pub async fn pick_all(engine: &dyn DecisionEngine, pending: &[Pending], already: &BTreeMap<String, &'static str>) -> Result<Vec<Pick>, EngineError> {
    // Round 1: narrow long lists to one category.
    let mut narrowed: Vec<Vec<&Candidate>> = pending.iter().map(|p| p.candidates.iter().collect()).collect();
    // Set when round 1 already decided the outcome.
    let mut early: Vec<Option<Pick>> = (0..pending.len()).map(|_| None).collect();
    let mut round1 = BTreeMap::new();
    for (i, p) in pending.iter().enumerate() {
        if p.candidates.len() <= MAX_DIRECT {
            continue;
        }
        // "contact: email address, phone number", so the model knows what each holds.
        let mut cats: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for c in &p.candidates {
            let facts = cats.entry(category(c.fact).into()).or_default();
            if !facts.contains(&describe(c.fact)) {
                facts.push(describe(c.fact));
            }
        }
        let mut cats: BTreeMap<String, String> = cats.into_iter().map(|(k, v)| (k.clone(), format!("{k}: {}", v.join(", ")))).collect();
        cats.insert(NONE.into(), "none of these".into());
        round1.insert(format!("f{i}"), choice(&p.field, already, "What kind of information does `field` ask for?", &cats));
    }
    if !round1.is_empty() {
        let answers = ask(engine, round1).await?;
        for (i, p) in pending.iter().enumerate() {
            let Some((cat, prob)) = chosen(answers.get(&format!("f{i}"))) else { continue };
            if cat == NONE {
                early[i] = Some(Pick::NoneFits);
            } else if prob < ACCEPT_P {
                early[i] = Some(Pick::LowConfidence);
            } else {
                narrowed[i] = p.candidates.iter().filter(|c| category(c.fact) == cat).collect();
            }
        }
    }

    // Round 2: pick the value.
    let mut round2 = BTreeMap::new();
    for (i, p) in pending.iter().enumerate() {
        if early[i].is_some() || narrowed[i].is_empty() {
            continue;
        }
        let mut opts: BTreeMap<String, String> = narrowed[i]
            .iter()
            .enumerate()
            .map(|(j, c)| (format!("c{j}"), format!("{} ({})", c.value, describe(c.fact))))
            .collect();
        opts.insert(NONE.into(), "none of these".into());
        round2.insert(format!("f{i}"), choice(&p.field, already, "Which of the applicant's values belongs in `field`?", &opts));
    }
    let answers = if round2.is_empty() { BTreeMap::new() } else { ask(engine, round2).await? };

    Ok(early
        .into_iter()
        .enumerate()
        .map(|(i, early)| {
            if let Some(p) = early {
                return p;
            }
            if narrowed[i].is_empty() {
                return Pick::NoneFits;
            }
            match chosen(answers.get(&format!("f{i}"))) {
                None => Pick::LowConfidence,
                Some((NONE, _)) => Pick::NoneFits,
                Some((_, p)) if p < ACCEPT_P => Pick::LowConfidence,
                Some((id, _)) => id
                    .strip_prefix('c')
                    .and_then(|j| j.parse::<usize>().ok())
                    .and_then(|j| narrowed[i].get(j))
                    .map_or(Pick::LowConfidence, |c| Pick::Chosen((*c).clone())),
            }
        })
        .collect())
}
