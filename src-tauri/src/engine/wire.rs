//! Mapping between the public types and the llama.cpp `POST /v1/systemone` JSON.
use super::types::{Answer, EngineError, Question};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

pub fn wire_request(state: &str, questions: &BTreeMap<String, Question>) -> Value {
    let qs: Map<String, Value> = questions
        .iter()
        .map(|(id, q)| (id.clone(), wire_question(q)))
        .collect();
    json!({ "state": state, "questions": qs })
}

fn wire_question(q: &Question) -> Value {
    match q {
        Question::Noul { instructions } => json!({"type": "noul", "instructions": instructions}),
        Question::Choice {
            instructions,
            options,
        } => {
            let criteria: Map<String, Value> =
                options.iter().map(|o| (o.clone(), Value::Null)).collect();
            json!({"type": "choice", "instructions": instructions, "criteria": criteria})
        }
        Question::Score {
            instructions,
            legend,
        } => {
            // BTreeMap iterates in key order: lowest level first.
            let criteria: Vec<&String> = legend.values().collect();
            json!({"type": "score", "instructions": instructions, "criteria": criteria})
        }
    }
}

pub fn parse_wire_response(v: Value) -> Result<BTreeMap<String, Answer>, EngineError> {
    if let Some(err) = v.get("error") {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(if msg.contains("too large to process") {
            EngineError::ContextOverflow
        } else {
            EngineError::BadResponse(msg.to_string())
        });
    }
    let answers = v
        .get("answers")
        .and_then(Value::as_object)
        .ok_or_else(|| bad("missing `answers` object"))?;
    answers
        .iter()
        .map(|(id, a)| Ok((id.clone(), parse_answer(id, a)?)))
        .collect()
}

fn bad(msg: &str) -> EngineError {
    EngineError::BadResponse(msg.to_string())
}

fn parse_answer(id: &str, a: &Value) -> Result<Answer, EngineError> {
    let f = |key: &str| {
        a.get(key)
            .and_then(Value::as_f64)
            .ok_or_else(|| bad(&format!("answer `{id}`: missing number `{key}`")))
    };
    let probs = || -> Result<BTreeMap<String, f64>, EngineError> {
        a.get("probabilities")
            .and_then(Value::as_object)
            .ok_or_else(|| bad(&format!("answer `{id}`: missing `probabilities`")))?
            .iter()
            .map(|(k, v)| {
                v.as_f64()
                    .map(|p| (k.clone(), p))
                    .ok_or_else(|| bad(&format!("answer `{id}`: non-numeric probability `{k}`")))
            })
            .collect()
    };
    match a.get("type").and_then(Value::as_str) {
        Some("noul") => Ok(Answer::Noul { p: f("noul")? }),
        Some("choice") => {
            let choice = a
                .get("choice")
                .and_then(Value::as_str)
                .ok_or_else(|| bad(&format!("answer `{id}`: missing `choice`")))?
                .to_string();
            Ok(Answer::Choice {
                choice,
                probabilities: probs()?,
            })
        }
        Some("score") => Ok(Answer::Score {
            score: f("score")?,
            probabilities: probs()?,
        }),
        other => Err(bad(&format!("answer `{id}`: unknown type {other:?}"))),
    }
}
