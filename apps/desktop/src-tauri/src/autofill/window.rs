//! The apply window and the one command its remote page may call.
//! `capabilities/apply.json` grants `autofill_bridge` to `apply-*` windows on
//! any HTTPS page. Fills only reach hosts the user agreed to (see
//! [`host_decision`]); docs/superpowers/specs/2026-10-19-universal-autofill-design.md.

use base64::Engine as _;
use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::field::{FieldKind, FieldSpec, FillAction};
use crate::commands::AppError;
use crate::engine::DecisionEngine;
use crate::state::AppState;
use cvd_core::autofill::session::*;

const INIT_SCRIPT: &str = include_str!("inject.js");
const LABEL_PREFIX: &str = "apply-";
/// Whole message cap; a real form is well under this.
/// A rejected field is retried this many times before it goes to the user.
const MAX_RETRIES: usize = 2;
const MAX_MESSAGE_BYTES: usize = 400_000;
const MAX_RESUME_BYTES: u64 = 10 * 1024 * 1024;

fn job_id_of(label: &str) -> Option<i64> {
    label.strip_prefix(LABEL_PREFIX)?.parse().ok()
}

/// Opens (or focuses) the apply window for a job. `false` means the posting
/// isn't an HTTPS page and the caller should open it in the browser instead.
pub async fn start_apply(app: &AppHandle, st: &AppState, job_id: i64) -> Result<bool, AppError> {
    let job = crate::commands::get_job(st, job_id).await?;
    if !is_openable_url(&job.url) {
        return Ok(false);
    }
    let title = match (job.title.trim(), job.company.trim()) {
        ("", "") => "Apply".to_string(),
        (t, "") => format!("Apply · {t}"),
        (t, c) => format!("Apply · {t} at {c}"),
    };
    open_window(app, job_id, &job.url, &title)?;
    Ok(true)
}

pub fn open_window(app: &AppHandle, job_id: i64, url: &str, title: &str) -> Result<(), AppError> {
    let label = format!("{LABEL_PREFIX}{job_id}");
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.unminimize();
        return w.set_focus().map_err(|e| AppError::Io(e.to_string()));
    }
    if let Some(sessions) = app.try_state::<Sessions>() {
        sessions.reset(&label);
        // Opening the posting is consent for its own host.
        let mut session = Session::default();
        session.allowed.extend(https_host(url));
        sessions.put(&label, session);
    }
    let parsed = url
        .parse()
        .map_err(|_| AppError::NotFound(format!("Not a valid URL: {url}")))?;
    WebviewWindowBuilder::new(app, label, WebviewUrl::External(parsed))
        .title(title)
        .inner_size(1100.0, 900.0)
        .initialization_script_for_all_frames(INIT_SCRIPT)
        .build()
        .map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PageMessage {
    Fields {
        fields: Vec<FieldSpec>,
    },
    /// The page didn't keep a value cvd set.
    Rejected {
        key: String,
        value: String,
    },
    /// The user changed a field. The value is compared to facts, never stored.
    UserEdit {
        key: String,
        value: String,
    },
    /// The page shows an application confirmation after the user submitted.
    Submitted,
    Status {
        filled: usize,
        needs: Vec<Value>,
    },
    #[serde(other)]
    Other,
}

/// Sessions by window label.
#[derive(Default)]
pub struct Sessions(Mutex<HashMap<String, Session>>);

/// The newest exported CV for the job that still exists on disk. The path
/// comes from cvd's own database, never from the page.
async fn load_resume(st: &AppState, job_id: i64) -> Option<Resume> {
    for cv in st.store.list_cvs(job_id).await.ok()? {
        let Some(path) = cv.file_ref else { continue };
        let path = std::path::PathBuf::from(path);
        let Ok(meta) = tokio::fs::metadata(&path).await else {
            continue;
        };
        if !meta.is_file() || meta.len() > MAX_RESUME_BYTES {
            continue;
        }
        let Ok(bytes) = tokio::fs::read(&path).await else {
            continue;
        };
        let name = path.file_name()?.to_string_lossy().into_owned();
        return Some(Resume {
            name,
            b64: base64::engine::general_purpose::STANDARD.encode(bytes),
        });
    }
    None
}

async fn active_engine(st: &AppState) -> Option<Arc<dyn DecisionEngine>> {
    let model = st.active_model().await.ok()?;
    st.engine_for(&model).ok()
}

pub async fn handle_bridge(
    window: &WebviewWindow,
    st: &AppState,
    sessions: &Sessions,
    msg: Value,
) -> Result<(), AppError> {
    let label = window.label().to_string();
    let job_id =
        job_id_of(&label).ok_or_else(|| AppError::NotFound("Not an apply window".into()))?;
    let url = window.url().map_err(|e| AppError::Io(e.to_string()))?;
    if msg.to_string().len() > MAX_MESSAGE_BYTES {
        return Err(AppError::Io("Message too large".into()));
    }
    // The top page, then the frame the fields live in (set by the top frame's
    // relay from the browser's `event.origin`). Both must be allowed.
    let top = https_host(url.as_str())
        .ok_or_else(|| AppError::NotFound("Autofill only works on HTTPS pages".into()))?;
    let mut hosts = vec![top];
    if let Some(frame) = msg
        .get("origin")
        .and_then(Value::as_str)
        .and_then(https_host)
    {
        if !hosts.contains(&frame) {
            hosts.push(frame);
        }
    }
    let all_allowed = |s: &Session| {
        hosts
            .iter()
            .all(|h| host_decision(s, h) == HostDecision::Allowed)
    };
    let fields = match serde_json::from_value(msg) {
        Ok(PageMessage::Fields { fields }) => {
            if !consent(window, sessions, &label, &hosts).await {
                return Ok(());
            }
            fields
        }
        Ok(PageMessage::Rejected { key, value }) => {
            let mut session = sessions.get(&label);
            if !all_allowed(&session) {
                return Ok(());
            }
            let tries = {
                let ex = session.excluded.entry(key.clone()).or_default();
                ex.push(value);
                ex.len()
            };
            let field = session.fields.get(&key).cloned();
            sessions.put(&label, session);
            match field {
                Some(f) if tries <= MAX_RETRIES => vec![f],
                _ => {
                    let reason = "The page rejected this value".to_string();
                    return send(window, &[FillAction::NeedsUser { key, reason }]);
                }
            }
        }
        Ok(PageMessage::UserEdit { key, value }) => {
            let session = sessions.get(&label);
            if !all_allowed(&session) {
                return Ok(());
            }
            if let Some(field) = session.fields.get(&key) {
                let facts = crate::commands::apply_facts(st).await?;
                learn(
                    st,
                    field,
                    &value,
                    &facts,
                    session.filled.contains_key(&key),
                    &session.memory,
                )
                .await;
            }
            return Ok(());
        }
        Ok(PageMessage::Submitted) => {
            let mut session = sessions.get(&label);
            if session.submitted {
                return Ok(());
            }
            session.submitted = true;
            sessions.put(&label, session);
            let text = mark_applied(st, job_id).await;
            return notice(window, &text);
        }
        Ok(PageMessage::Status { filled, needs }) => {
            #[cfg(debug_assertions)]
            eprintln!(
                "[autofill] status: {filled} filled, {} need the user: {}",
                needs.len(),
                Value::from(needs.clone())
            );
            let _ = (filled, needs);
            return Ok(());
        }
        _ => return Ok(()),
    };
    #[cfg(debug_assertions)]
    if let Ok(dir) = std::env::var("CVD_AUTOFILL_DUMP") {
        let file = format!(
            "{dir}/{}-{}.json",
            url.host_str().unwrap_or("page"),
            chrono::Local::now().format("%H%M%S%3f")
        );
        let _ = std::fs::write(
            file,
            serde_json::to_string_pretty(
                &serde_json::json!({ "url": url.as_str(), "fields": fields_json(&fields) }),
            )
            .unwrap_or_default(),
        );
    }
    let facts = crate::commands::apply_facts(st).await?;
    let wants_file = fields.iter().any(|f| f.kind == Some(FieldKind::File));
    let resume = if wants_file {
        load_resume(st, job_id).await
    } else {
        None
    };
    let engine = active_engine(st).await;
    let mut session = sessions.get(&label);
    session.memory = st.store.apply_memory().await.unwrap_or_default();
    let actions = plan(
        fields,
        &facts,
        resume.as_ref(),
        engine.as_deref(),
        &mut session,
    )
    .await;
    sessions.put(&label, session);
    #[cfg(debug_assertions)]
    for a in &actions {
        match a {
            FillAction::Set { key, value, fact } => {
                eprintln!("[autofill] {key}: set {fact} = {value}")
            }
            FillAction::File { key, name, .. } => eprintln!("[autofill] {key}: attach {name}"),
            FillAction::NeedsUser { key, reason } => {
                eprintln!("[autofill] {key}: needs user ({reason})")
            }
        }
    }
    send(window, &actions)
}

/// Asks the user, in a native dialog the page can't draw or answer, before a
/// new host receives any profile data. `false` means don't fill (declined, or
/// a dialog is already open).
async fn consent(
    window: &WebviewWindow,
    sessions: &Sessions,
    label: &str,
    hosts: &[String],
) -> bool {
    for host in hosts {
        let session = sessions.get(label);
        match host_decision(&session, host) {
            HostDecision::Allowed => continue,
            HostDecision::Denied => return false,
            HostDecision::Ask if session.asking => return false,
            HostDecision::Ask => {}
        }
        let mut session = session;
        session.asking = true;
        sessions.put(label, session);
        let ok = ask_host(window, host).await;
        let mut session = sessions.get(label);
        session.asking = false;
        if ok {
            session.allowed.insert(host.clone());
        } else {
            session.denied.insert(host.clone());
        }
        sessions.put(label, session);
        if !ok {
            let _ = notice(
                window,
                &format!("Autofill is off on {host}. Reopen the application to change that."),
            );
            return false;
        }
    }
    true
}

async fn ask_host(window: &WebviewWindow, host: &str) -> bool {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let (tx, rx) = tokio::sync::oneshot::channel();
    window
        .dialog()
        .message(format!(
            "{host} is asking for application details. cvd can fill this form with your profile (name, contact details, links, CV). Only allow this on sites you trust."
        ))
        .title(format!("Autofill on {host}?"))
        .kind(MessageDialogKind::Info)
        .buttons(MessageDialogButtons::OkCancelCustom("Fill this form".into(), "Not now".into()))
        .parent(window)
        .show(move |ok| {
            let _ = tx.send(ok);
        });
    rx.await.unwrap_or(false)
}

#[cfg(debug_assertions)]
fn fields_json(fields: &[FieldSpec]) -> Value {
    fields
        .iter()
        .map(|f| {
            serde_json::json!({
                "key": f.key, "kind": f.kind(), "label": f.label, "placeholder": f.placeholder,
                "autocomplete": f.autocomplete, "name_attr": f.name_attr, "help": f.help,
                "section": f.section, "required": f.required, "options": f.options,
            })
        })
        .collect()
}

fn notice(window: &WebviewWindow, text: &str) -> Result<(), AppError> {
    let json = serde_json::to_string(text).map_err(|e| AppError::Io(e.to_string()))?;
    window
        .eval(format!(
            "window.__cvdAutofill && window.__cvdAutofill.notice({json})"
        ))
        .map_err(|e| AppError::Io(e.to_string()))
}

fn send(window: &WebviewWindow, actions: &[FillAction]) -> Result<(), AppError> {
    let json = serde_json::to_string(actions).map_err(|e| AppError::Io(e.to_string()))?;
    window
        .eval(format!(
            "window.__cvdAutofill && window.__cvdAutofill.apply({json})"
        ))
        .map_err(|e| AppError::Io(e.to_string()))
}

impl Sessions {
    fn get(&self, label: &str) -> Session {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(label)
            .cloned()
            .unwrap_or_default()
    }

    fn put(&self, label: &str, session: Session) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(label.to_string(), session);
    }

    /// A reopened window starts fresh.
    pub fn reset(&self, label: &str) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(label);
    }
}
