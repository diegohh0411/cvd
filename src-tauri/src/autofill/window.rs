//! The apply window and the one command its remote page may call.
//! `capabilities/apply.json` grants `autofill_bridge` to `apply-*` windows on
//! the hosts in [`SUPPORTED_HOSTS`]; the handler re-checks both.

use base64::Engine as _;
use serde::Deserialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::field::{FillAction, FieldSpec, MAX_FIELDS};
use super::matcher::{decide, Decision};
use super::Fact;
use crate::commands::AppError;
use crate::state::AppState;

const INIT_SCRIPT: &str = include_str!("inject.js");
const LABEL_PREFIX: &str = "apply-";
/// Whole message cap; a real form is well under this.
const MAX_MESSAGE_BYTES: usize = 400_000;
const MAX_RESUME_BYTES: u64 = 10 * 1024 * 1024;

/// Hosts the bridge works on. Keep in sync with `capabilities/apply.json`,
/// which Tauri fixes at build time.
pub const SUPPORTED_HOSTS: &[&str] = &["greenhouse.io", "jobs.lever.co", "jobs.ashbyhq.com"];

pub fn is_supported_url(url: &str) -> bool {
    let Ok(u) = tauri::Url::parse(url) else { return false };
    if u.scheme() != "https" {
        return false;
    }
    let host = u.host_str().unwrap_or("");
    SUPPORTED_HOSTS.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

fn job_id_of(label: &str) -> Option<i64> {
    label.strip_prefix(LABEL_PREFIX)?.parse().ok()
}

/// Opens (or focuses) the apply window for a job. `false` means the posting's
/// host isn't supported and the caller should open it in the browser instead.
pub async fn start_apply(app: &AppHandle, st: &AppState, job_id: i64) -> Result<bool, AppError> {
    let job = crate::commands::get_job(st, job_id).await?;
    if !is_supported_url(&job.url) {
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
    let parsed = url.parse().map_err(|_| AppError::NotFound(format!("Not a valid URL: {url}")))?;
    WebviewWindowBuilder::new(app, label, WebviewUrl::External(parsed))
        .title(title)
        .inner_size(1100.0, 900.0)
        .initialization_script(INIT_SCRIPT)
        .build()
        .map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PageMessage {
    Fields { fields: Vec<FieldSpec> },
    Status { filled: usize, needs: Vec<Value> },
    #[serde(other)]
    Other,
}

pub struct Resume {
    pub name: String,
    pub b64: String,
}

/// Turns the page's fields into fill actions. Pure, so the harness is testable
/// without a webview.
pub fn plan(fields: Vec<FieldSpec>, facts: &[Fact], resume: Option<&Resume>) -> Vec<FillAction> {
    fields
        .into_iter()
        .take(MAX_FIELDS)
        .map(FieldSpec::sanitized)
        .filter(|f| !f.key.is_empty())
        .map(|f| match decide(&f, facts) {
            Decision::Fill { fact, value } => FillAction::Set { key: f.key, value, fact },
            Decision::Resume => match resume {
                Some(r) => FillAction::File { key: f.key, name: r.name.clone(), b64: r.b64.clone() },
                None => FillAction::NeedsUser { key: f.key, reason: "Export a CV for this job first, or attach one yourself".into() },
            },
            Decision::NeedsUser(reason) => FillAction::NeedsUser { key: f.key, reason },
        })
        .collect()
}

/// The newest exported CV for the job that still exists on disk. The path
/// comes from cvd's own database, never from the page.
async fn load_resume(st: &AppState, job_id: i64) -> Option<Resume> {
    for cv in st.store.list_cvs(job_id).await.ok()? {
        let Some(path) = cv.pdf_path else { continue };
        let path = std::path::PathBuf::from(path);
        let Ok(meta) = tokio::fs::metadata(&path).await else { continue };
        if !meta.is_file() || meta.len() > MAX_RESUME_BYTES {
            continue;
        }
        let Ok(bytes) = tokio::fs::read(&path).await else { continue };
        let name = path.file_name()?.to_string_lossy().into_owned();
        return Some(Resume { name, b64: base64::engine::general_purpose::STANDARD.encode(bytes) });
    }
    None
}

pub async fn handle_bridge(window: &WebviewWindow, st: &AppState, msg: Value) -> Result<(), AppError> {
    let label = window.label();
    let job_id = job_id_of(label).ok_or_else(|| AppError::NotFound("Not an apply window".into()))?;
    let url = window.url().map_err(|e| AppError::Io(e.to_string()))?;
    if !is_supported_url(url.as_str()) {
        return Err(AppError::NotFound("Autofill is not available on this page".into()));
    }
    if msg.to_string().len() > MAX_MESSAGE_BYTES {
        return Err(AppError::Io("Message too large".into()));
    }
    let fields = match serde_json::from_value(msg) {
        Ok(PageMessage::Fields { fields }) => fields,
        Ok(PageMessage::Status { filled, needs }) => {
            #[cfg(debug_assertions)]
            eprintln!("[autofill] status: {filled} filled, {} need the user: {}", needs.len(), Value::from(needs.clone()));
            let _ = (filled, needs);
            return Ok(());
        }
        _ => return Ok(()),
    };
    let facts = crate::commands::apply_facts(st).await?;
    let wants_file = fields.iter().any(|f| f.kind == Some(super::field::FieldKind::File));
    let resume = if wants_file { load_resume(st, job_id).await } else { None };
    let actions = plan(fields, &facts, resume.as_ref());
    #[cfg(debug_assertions)]
    for a in &actions {
        match a {
            FillAction::Set { key, value, fact } => eprintln!("[autofill] {key}: set {fact} = {value}"),
            FillAction::File { key, name, .. } => eprintln!("[autofill] {key}: attach {name}"),
            FillAction::NeedsUser { key, reason } => eprintln!("[autofill] {key}: needs user ({reason})"),
        }
    }
    let json = serde_json::to_string(&actions).map_err(|e| AppError::Io(e.to_string()))?;
    window
        .eval(format!("window.__cvdAutofill && window.__cvdAutofill.apply({json})"))
        .map_err(|e| AppError::Io(e.to_string()))
}

/// The single command remote pages may call (see `capabilities/apply.json`).
#[tauri::command]
pub async fn autofill_bridge(window: WebviewWindow, st: tauri::State<'_, AppState>, msg: Value) -> Result<(), AppError> {
    handle_bridge(&window, &st, msg).await
}
