//! Question builders and state formatting for the matching pipeline.

use std::collections::BTreeMap;

use crate::engine::Question;
use crate::store::types::Entry;

pub const SENIORITY_KEY: &str = "seniority";
pub const ROLE_FAMILY_KEY: &str = "role_family";
pub const RELEVANCE_KEY: &str = "relevance";
pub const SENIORITY_FIT_KEY: &str = "seniority_fit";

pub const SENIORITY_OPTIONS: [&str; 4] = ["junior", "mid", "senior", "lead"];
pub const ROLE_FAMILY_OPTIONS: [&str; 7] = [
    "backend",
    "frontend",
    "fullstack",
    "data/ml",
    "devops/infra",
    "mobile",
    "other",
];

pub const ENTRY_INSTRUCTION: &str = "How relevant is this experience to the job description?";
pub const BULLET_INSTRUCTION: &str =
    "How strongly does this bullet support a fit for the job description?";
pub const SENIORITY_INSTRUCTION: &str = "What seniority level does this role target?";
pub const ROLE_FAMILY_INSTRUCTION: &str = "Which role family does this job belong to?";
pub const SENIORITY_FIT_INSTRUCTION: &str =
    "Does this experience show the seniority the job description needs?";

pub fn skill_key(id: i64) -> String {
    format!("skill:{id}")
}

pub fn bullet_key(id: i64) -> String {
    format!("bullet:{id}")
}

pub fn skill_question(name: &str) -> Question {
    Question::Noul {
        instructions: format!("Does this role require or value the skill \"{name}\"?"),
    }
}

pub fn seniority_question() -> Question {
    Question::Choice {
        instructions: SENIORITY_INSTRUCTION.into(),
        options: SENIORITY_OPTIONS.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn role_family_question() -> Question {
    Question::Choice {
        instructions: ROLE_FAMILY_INSTRUCTION.into(),
        options: ROLE_FAMILY_OPTIONS.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn relevance_legend() -> BTreeMap<u8, String> {
    [
        (0, "irrelevant"),
        (1, "tangential"),
        (2, "somewhat relevant"),
        (3, "relevant"),
        (4, "highly relevant"),
    ]
    .into_iter()
    .map(|(k, v)| (k, v.to_string()))
    .collect()
}

pub fn entry_question() -> Question {
    Question::Score {
        instructions: ENTRY_INSTRUCTION.into(),
        legend: relevance_legend(),
    }
}

pub fn seniority_fit_question() -> Question {
    Question::Noul {
        instructions: SENIORITY_FIT_INSTRUCTION.into(),
    }
}

pub fn bullet_question() -> Question {
    Question::Score {
        instructions: BULLET_INSTRUCTION.into(),
        legend: relevance_legend(),
    }
}

pub fn jd_state(jd: &str) -> String {
    format!("JOB DESCRIPTION:\n{jd}")
}

pub fn entry_state(jd: &str, e: &Entry) -> String {
    let end = e.end_date.as_deref().unwrap_or("present");
    let mut s = format!(
        "JOB DESCRIPTION:\n{jd}\n\nCANDIDATE ENTRY:\n{} — {} ({}–{end})\n{}\n",
        e.title, e.org, e.start_date, e.description
    );
    for b in &e.bullets {
        s.push_str(&format!("- {}\n", b.text));
    }
    let skills: Vec<&str> = e.skills.iter().map(|k| k.name.as_str()).collect();
    s.push_str(&format!("Skills: {}", skills.join(", ")));
    s
}
