//! System One questions for one email, and the parsing of their answers.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::decide::Classified;
use crate::engine::{decide_checked, Answer, DecisionEngine, EngineError, Question};
use crate::store::types::JobStatus;

pub const BODY_CHARS: usize = 6000;

pub const JOB_RELATED_KEY: &str = "job_related";
pub const MATCHED_JOB_KEY: &str = "matched_job";
pub const STATUS_CHANGED_KEY: &str = "status_changed";
pub const NEW_STATUS_KEY: &str = "new_status";
pub const NO_MATCH_KEY: &str = "no_match";

pub struct EmailInput {
    pub subject: String,
    pub sender: String,
    pub body: String,
}

impl EmailInput {
    pub fn new(subject: &str, sender: &str, body: &str) -> Self {
        Self {
            subject: subject.to_string(),
            sender: sender.to_string(),
            body: body.chars().take(BODY_CHARS).collect(),
        }
    }
}

pub struct JobCtx {
    pub id: i64,
    pub company: String,
    pub title: String,
    pub url: String,
    pub status: Option<String>,
}

/// Same label the Jobs page shows: "Company — Title", or "Untitled job".
pub fn job_label(company: &str, title: &str) -> String {
    let parts: Vec<&str> = [company, title]
        .into_iter()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        "Untitled job".to_string()
    } else {
        parts.join(" — ")
    }
}

fn job_key(id: i64) -> String {
    format!("job_{id}")
}

fn status_key(id: i64) -> String {
    format!("status_{id}")
}

/// What each status means for an email, keyed by lowercase status name.
/// Statuses are user-editable, so unknown names are sent undescribed.
fn status_hint(name: &str) -> Option<&'static str> {
    match name.trim().to_lowercase().as_str() {
        "bookmarked" | "saved" => Some("Saved, but the user has not applied yet"),
        "applied" => Some("The user has submitted an application: a confirmation that it was submitted, sent or received, or that it is under review"),
        "interview" | "interviewing" => Some("The user is invited to, or scheduled for, an interview, call or assessment"),
        "offer" => Some("The user received a job offer"),
        "rejected" => Some("The company turned the user down, or the application was closed without an offer"),
        _ => None,
    }
}

pub fn build_questions(jobs: &[JobCtx], statuses: &[JobStatus]) -> BTreeMap<String, Question> {
    let mut job_criteria: BTreeMap<String, String> = jobs
        .iter()
        .map(|j| (job_key(j.id), job_label(&j.company, &j.title)))
        .collect();
    job_criteria.insert(NO_MATCH_KEY.to_string(), "None of the listed jobs".to_string());

    let status_criteria: BTreeMap<String, String> = statuses
        .iter()
        .map(|s| {
            let desc = match status_hint(&s.name) {
                Some(hint) => format!("{}: {hint}", s.name),
                None => s.name.clone(),
            };
            (status_key(s.id), desc)
        })
        .collect();

    let mut questions = BTreeMap::new();
    questions.insert(
        JOB_RELATED_KEY.to_string(),
        Question::Noul {
            instructions: "The email concerns the user's own job search: an application confirmation (including automated ones from applicant-tracking systems), an interview, an assessment, an offer, a rejection, or recruiter contact. Newsletters, job alerts and marketing do not count.".into(),
        },
    );
    questions.insert(
        MATCHED_JOB_KEY.to_string(),
        Question::Choice {
            instructions: "Which job in `jobs` does the email refer to? Match on company and role title.".into(),
            options: job_criteria.keys().cloned().collect(),
            criteria: job_criteria,
        },
    );
    questions.insert(
        STATUS_CHANGED_KEY.to_string(),
        Question::Noul {
            instructions: "The email shows the matched job has moved past the `status` it has in `jobs`. For example, an application-submitted confirmation for a job that is still bookmarked, an interview invitation, an offer or a rejection. It does not hold if the email only repeats what that status already says.".into(),
        },
    );
    questions.insert(
        NEW_STATUS_KEY.to_string(),
        Question::Choice {
            instructions: "What is the status of the matched job after this email?".into(),
            options: status_criteria.keys().cloned().collect(),
            criteria: status_criteria,
        },
    );
    questions
}

pub fn build_state(email: &EmailInput, jobs: &[JobCtx]) -> String {
    let jobs: Vec<Value> = jobs
        .iter()
        .map(|j| {
            json!({
                "id": j.id,
                "company": j.company,
                "title": j.title,
                "url": j.url,
                "status": j.status,
            })
        })
        .collect();
    json!({
        "email": { "subject": email.subject, "sender": email.sender, "body": email.body },
        "jobs": jobs,
    })
    .to_string()
}

pub async fn classify(
    engine: &dyn DecisionEngine,
    email: &EmailInput,
    jobs: &[JobCtx],
    statuses: &[JobStatus],
) -> Result<Classified, EngineError> {
    let questions = build_questions(jobs, statuses);
    let state = build_state(email, jobs);
    let answers = decide_checked(engine, &state, &questions).await?;

    Ok(Classified {
        job_related: yes_probability(&answers, JOB_RELATED_KEY)?,
        matched_job: matched_job(&answers, jobs)?,
        status_changed: yes_probability(&answers, STATUS_CHANGED_KEY)?,
        new_status: Some(new_status(&answers, statuses)?),
    })
}

fn answer<'a>(answers: &'a BTreeMap<String, Answer>, key: &str) -> Result<&'a Answer, EngineError> {
    answers
        .get(key)
        .ok_or_else(|| EngineError::BadResponse(format!("missing answer for {key}")))
}

fn yes_probability(answers: &BTreeMap<String, Answer>, key: &str) -> Result<f64, EngineError> {
    match answer(answers, key)? {
        Answer::Noul { p } => Ok(*p),
        _ => Err(EngineError::BadResponse(format!("expected a yes/no answer for {key}"))),
    }
}

fn choice_answer<'a>(
    answers: &'a BTreeMap<String, Answer>,
    key: &str,
) -> Result<(&'a str, &'a BTreeMap<String, f64>), EngineError> {
    match answer(answers, key)? {
        Answer::Choice { choice, probabilities } => Ok((choice.as_str(), probabilities)),
        _ => Err(EngineError::BadResponse(format!("expected a choice for {key}"))),
    }
}

/// Unknown option keys are rejected, never mapped to a nearby job or status.
fn matched_job(answers: &BTreeMap<String, Answer>, jobs: &[JobCtx]) -> Result<Option<i64>, EngineError> {
    let (choice, _) = choice_answer(answers, MATCHED_JOB_KEY)?;
    if choice == NO_MATCH_KEY {
        return Ok(None);
    }
    jobs.iter()
        .find(|j| job_key(j.id) == choice)
        .map(|j| Some(j.id))
        .ok_or_else(|| EngineError::BadResponse(format!("unknown job option {choice}")))
}

/// Confidence is the probability the engine gave the chosen status.
fn new_status(answers: &BTreeMap<String, Answer>, statuses: &[JobStatus]) -> Result<(i64, f64), EngineError> {
    let (choice, probabilities) = choice_answer(answers, NEW_STATUS_KEY)?;
    let status = statuses
        .iter()
        .find(|s| status_key(s.id) == choice)
        .ok_or_else(|| EngineError::BadResponse(format!("unknown status option {choice}")))?;
    let confidence = probabilities
        .get(choice)
        .copied()
        .ok_or_else(|| EngineError::BadResponse(format!("no probability for {choice}")))?;
    Ok((status.id, confidence))
}
