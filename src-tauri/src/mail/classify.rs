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

fn choice_instructions(question: &str, options: &BTreeMap<String, String>) -> Value {
    json!({ "question": question, "options": options })
}

pub fn build_questions(jobs: &[JobCtx], statuses: &[JobStatus]) -> BTreeMap<String, Question> {
    let mut job_options: BTreeMap<String, String> = jobs
        .iter()
        .map(|j| (job_key(j.id), job_label(&j.company, &j.title)))
        .collect();
    job_options.insert(NO_MATCH_KEY.to_string(), "No matching job".to_string());

    let status_options: BTreeMap<String, String> = statuses
        .iter()
        .map(|s| (status_key(s.id), s.name.clone()))
        .collect();

    let mut questions = BTreeMap::new();
    questions.insert(
        JOB_RELATED_KEY.to_string(),
        Question::Noul {
            instructions: "Is this email about the user's job search? Yes for application confirmations (e.g. \"your application was submitted/sent/received\", including automated ones from applicant-tracking systems such as Workable, Greenhouse, Lever or LinkedIn), interview invitations, assessments, offers, rejections and recruiter contact. No for newsletters, job alerts, marketing and unrelated mail.".into(),
        },
    );
    questions.insert(
        MATCHED_JOB_KEY.to_string(),
        Question::Choice {
            instructions: choice_instructions("Which job does this email refer to?", &job_options),
            options: job_options.keys().cloned().collect(),
        },
    );
    questions.insert(
        STATUS_CHANGED_KEY.to_string(),
        Question::Noul {
            instructions: "Compare what this email says with the matched job's current status in the jobs list. Answer yes if the email shows the user has progressed beyond that status, even if the email itself is an automated confirmation. In particular, a confirmation that the user's application was submitted, sent or received means the user applied, so it is a change for any job still saved/bookmarked/not yet applied. Also yes for interviews, offers and rejections. Answer no only if the email adds nothing beyond the job's current status (e.g. a newsletter, a generic reminder, or a repeat of the status the job already has)."
                .into(),
        },
    );
    questions.insert(
        NEW_STATUS_KEY.to_string(),
        Question::Choice {
            instructions: choice_instructions("Which status best describes the job after this email? An application submitted/sent/received confirmation means the applied status; interview invitations mean an interview status; an offer means offer; a rejection means rejected.", &status_options),
            options: status_options.keys().cloned().collect(),
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
