//! Shared application form planning, consent policy, and learning.
use super::field::{FieldKind, FieldSpec, FillAction, MAX_FIELDS};
use super::matcher::{decide_with, is_learnable, normalize, Decision};
use super::picker::{candidates, loosely_matches, pick_all, Pending, Pick};
use super::Fact;
use crate::{engine::DecisionEngine, state::AppState};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// ATS hosts cvd was tuned on. They fill without asking; any other host asks
/// the user first.
pub const KNOWN_ATS_HOSTS: &[&str] = &["greenhouse.io", "jobs.lever.co", "jobs.ashbyhq.com"];

/// The host of an `https` URL, lowercased. `None` for anything else.
pub fn https_host(url: &str) -> Option<String> {
    let u = reqwest::Url::parse(url).ok()?;
    if u.scheme() != "https" {
        return None;
    }
    u.host_str()
        .filter(|h| !h.is_empty())
        .map(str::to_ascii_lowercase)
}

/// Any HTTPS page can open in the apply window.
pub fn is_openable_url(url: &str) -> bool {
    https_host(url).is_some()
}

pub fn is_known_ats_host(host: &str) -> bool {
    KNOWN_ATS_HOSTS
        .iter()
        .any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

pub fn is_known_ats(url: &str) -> bool {
    https_host(url).is_some_and(|h| is_known_ats_host(&h))
}

#[derive(Debug, PartialEq, Eq)]
pub enum HostDecision {
    Allowed,
    Denied,
    Ask,
}

/// Whether `host` may receive fills in this window.
pub fn host_decision(session: &Session, host: &str) -> HostDecision {
    if session.denied.contains(host) {
        HostDecision::Denied
    } else if session.allowed.contains(host) || is_known_ats_host(host) {
        HostDecision::Allowed
    } else {
        HostDecision::Ask
    }
}

pub struct Resume {
    pub name: String,
    pub b64: String,
}

/// What one apply window has seen so far: fields by key (for retries), values
/// the page rejected, and which facts went into which labels.
#[derive(Default, Clone)]
pub struct Session {
    pub fields: HashMap<String, FieldSpec>,
    pub excluded: HashMap<String, Vec<String>>,
    pub already: BTreeMap<String, &'static str>,
    /// Learned label → fact keys, loaded from the store per page message.
    pub memory: BTreeMap<String, String>,
    /// Fields cvd filled, by key, with the fact it used.
    pub filled: HashMap<String, &'static str>,
    pub submitted: bool,
    /// Hosts the user agreed may receive fills (plus the posting's own host).
    pub allowed: BTreeSet<String>,
    /// Hosts the user declined; cvd stays quiet there.
    pub denied: BTreeSet<String>,
    /// A consent dialog is open for this window.
    pub asking: bool,
}

/// Turns the page's fields into fill actions: rules first, then the Fast Model
/// for fields no rule matched. With no engine, those fields go to the user.
pub async fn plan(
    fields: Vec<FieldSpec>,
    facts: &[Fact],
    resume: Option<&Resume>,
    engine: Option<&dyn DecisionEngine>,
    session: &mut Session,
) -> Vec<FillAction> {
    let mut actions: Vec<Option<FillAction>> = Vec::new();
    let mut pending: Vec<(usize, Pending, String)> = Vec::new();
    let fields: Vec<FieldSpec> = fields
        .into_iter()
        .take(MAX_FIELDS)
        .map(FieldSpec::sanitized)
        .filter(|f| !f.key.is_empty())
        .collect();
    for f in fields {
        session.fields.insert(f.key.clone(), f.clone());
        let excluded = session.excluded.get(&f.key).cloned().unwrap_or_default();
        let decision = match decide_with(&f, facts, &session.memory) {
            // A value the page rejected goes back to the model with that value excluded.
            Decision::Fill { value, .. } if excluded.contains(&value) => {
                Decision::Unsure("The page rejected this value".into())
            }
            d => d,
        };
        let action = match decision {
            Decision::Fill { fact, value } => {
                session.already.insert(f.label.clone(), fact);
                session.filled.insert(f.key.clone(), fact);
                Some(FillAction::Set {
                    key: f.key,
                    value,
                    fact,
                })
            }
            Decision::Resume => Some(match resume {
                Some(r) => FillAction::File {
                    key: f.key,
                    name: r.name.clone(),
                    b64: r.b64.clone(),
                },
                None => FillAction::NeedsUser {
                    key: f.key,
                    reason: "Export a CV for this job first, or attach one yourself".into(),
                },
            }),
            Decision::NeedsUser(reason) => Some(FillAction::NeedsUser { key: f.key, reason }),
            Decision::Unsure(reason) => {
                let cands = candidates(&f, facts, &excluded);
                if engine.is_none() || cands.is_empty() {
                    Some(FillAction::NeedsUser { key: f.key, reason })
                } else {
                    pending.push((
                        actions.len(),
                        Pending {
                            field: f,
                            candidates: cands,
                        },
                        reason,
                    ));
                    None
                }
            }
        };
        actions.push(action);
    }

    if let (Some(engine), false) = (engine, pending.is_empty()) {
        let batch: Vec<Pending> = pending
            .iter()
            .map(|(_, p, _)| Pending {
                field: p.field.clone(),
                candidates: p.candidates.clone(),
            })
            .collect();
        let picks = pick_all(engine, &batch, &session.already).await;
        for (n, (i, p, reason)) in pending.into_iter().enumerate() {
            let key = p.field.key.clone();
            actions[i] = Some(match picks.as_ref().map(|v| &v[n]) {
                Ok(Pick::Chosen(c)) => {
                    session.already.insert(p.field.label.clone(), c.fact);
                    session.filled.insert(key.clone(), c.fact);
                    FillAction::Set {
                        key,
                        value: c.value.clone(),
                        fact: c.fact,
                    }
                }
                Ok(Pick::NoneFits) => FillAction::NeedsUser { key, reason },
                Ok(Pick::LowConfidence) => FillAction::NeedsUser {
                    key,
                    reason: "Not sure enough to fill this".into(),
                },
                Err(e) => FillAction::NeedsUser {
                    key,
                    reason: format!("Fast Model unavailable: {e}"),
                },
            });
        }
    }
    actions.into_iter().flatten().collect()
}

/// The fact keys whose spellings match what the user typed or picked. Typed
/// text must match exactly; a picked option may carry extras ("Mexico (+52)").
/// Yes/no preferences are skipped: "Yes" says nothing about which one was meant.
pub fn facts_matching(value: &str, facts: &[Fact], picked: bool) -> Vec<&'static str> {
    let v = normalize(value);
    if v.is_empty() {
        return vec![];
    }
    let mut keys: Vec<&'static str> = facts
        .iter()
        .filter(|f| !super::matcher::is_yes_no(f.key))
        .filter(|f| {
            f.variants
                .iter()
                .any(|x| normalize(x) == v || (picked && v.len() >= 3 && loosely_matches(value, x)))
        })
        .map(|f| f.key)
        .collect();
    keys.dedup();
    keys
}

/// Learns from one edit: a value matching exactly one fact teaches cvd that
/// this label means that fact. Replacing a remembered fill with something else
/// makes cvd forget the label.
pub async fn learn(
    st: &AppState,
    field: &FieldSpec,
    value: &str,
    facts: &[Fact],
    cvd_filled: bool,
    memory: &BTreeMap<String, String>,
) {
    if !is_learnable(field) || value.len() > 500 {
        return;
    }
    let label = normalize(&field.label);
    let picked = matches!(
        field.kind(),
        FieldKind::Select | FieldKind::Radio | FieldKind::Combobox
    );
    match facts_matching(value, facts, picked)[..] {
        [key] => {
            let _ = st.store.remember_label(&label, key).await;
        }
        [] if cvd_filled && memory.contains_key(&label) => {
            let _ = st.store.forget_label(&label).await;
        }
        _ => {}
    }
}

/// Moves the job to the status named "Applied", if there is one. Returns the
/// line shown in the page.
pub async fn mark_applied(st: &AppState, job_id: i64) -> String {
    if job_id == 0 {
        return "Submitted. (Dev window: no job to update.)".into();
    }
    let Ok(statuses) = st.store.list_job_statuses().await else {
        return "Submitted. cvd couldn't read your statuses.".into();
    };
    let Some(applied) = statuses
        .iter()
        .find(|s| s.name.trim().eq_ignore_ascii_case("applied"))
    else {
        return "Submitted. Add an \"Applied\" status in cvd to track this automatically.".into();
    };
    let Ok(job) = crate::services::get_job(st, job_id).await else {
        return "Submitted. The job is no longer in cvd.".into();
    };
    if job.status_id != Some(applied.id)
        && crate::services::update_job_tracking(st, job_id, Some(applied.id), &job.url)
            .await
            .is_err()
    {
        return "Submitted. cvd couldn't update the job's status.".into();
    }
    (st.emitter())(
        crate::state::JOB_UPDATED_EVENT,
        serde_json::json!({ "job_id": job_id, "status_id": applied.id }),
    );
    format!("Submitted. Marked as {} in cvd.", applied.name)
}
