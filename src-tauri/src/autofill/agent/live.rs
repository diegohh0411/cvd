//! The vision agent on a real apply window: which windows use it, requests
//! to the page and their replies, and the [`Page`] the loop drives.
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, WebviewWindow};
use tokio::sync::{oneshot, Notify};

use super::capture;
use super::run::{run, Acted, End, Filled, Page, Screen};
use super::step::{Action, Mark};
use super::vision::{cache_key, supports_images};
use crate::autofill::field::{FieldSpec, FillAction};
use crate::autofill::window::{
    active_engine, consent, fill_fields, https_host, job_id_of, notice, send, Sessions,
};
use crate::state::AppState;

const MARKS_TIMEOUT: Duration = Duration::from_secs(15);
/// A click that navigates never answers; the next step re-marks the new page.
const ACT_TIMEOUT: Duration = Duration::from_secs(5);
/// Fields wait this long for the vision check before taking the DOM path.
const MODE_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_MARKS: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Checking whether the Fast Model can see.
    Pending,
    Vision,
    /// Today's DOM-only autofill.
    Dom,
}

#[derive(Default)]
struct Window {
    mode: Option<Mode>,
    running: bool,
    stop: Arc<AtomicBool>,
}

/// Vision agent state for every apply window, by label.
#[derive(Default)]
pub struct Agents {
    windows: Mutex<HashMap<String, Window>>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Value>>>,
    next_id: AtomicU64,
    changed: Notify,
}

impl Agents {
    fn with<R>(&self, label: &str, f: impl FnOnce(&mut Window) -> R) -> R {
        let mut map = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        f(map.entry(label.to_string()).or_default())
    }

    pub fn set_mode(&self, label: &str, mode: Mode) {
        self.with(label, |w| w.mode = Some(mode));
        self.changed.notify_waiters();
    }

    /// The window's mode once known. Windows cvd never checked are DOM-only.
    pub async fn mode(&self, label: &str) -> Mode {
        let deadline = tokio::time::Instant::now() + MODE_TIMEOUT;
        loop {
            let notified = self.changed.notified();
            match self.with(label, |w| w.mode) {
                Some(Mode::Pending) => {}
                Some(m) => return m,
                None => return Mode::Dom,
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return Mode::Dom;
            }
        }
    }

    /// Claims the window for a run; `None` if it isn't in vision mode or
    /// already running.
    fn begin(&self, label: &str) -> Option<Arc<AtomicBool>> {
        self.with(label, |w| {
            if w.mode != Some(Mode::Vision) || w.running {
                return None;
            }
            w.running = true;
            w.stop = Arc::default();
            Some(w.stop.clone())
        })
    }

    /// Ends a run. Later pages in the window use the DOM path: the agent
    /// handed over to the user.
    fn finish(&self, label: &str) {
        self.with(label, |w| w.running = false);
        self.set_mode(label, Mode::Dom);
    }

    pub fn stop(&self, label: &str) {
        self.with(label, |w| w.stop.store(true, Ordering::SeqCst));
    }

    pub fn reply(&self, id: u64, data: Value) {
        let tx = self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
        if let Some(tx) = tx {
            let _ = tx.send(data);
        }
    }

    fn expect(&self) -> (u64, oneshot::Receiver<Value>) {
        let (tx, rx) = oneshot::channel();
        // Not guessable from the page: mixes a counter with the clock.
        let n = self.next_id.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        // JS numbers are exact up to 2^53.
        let id = (nanos.wrapping_mul(0x9E37_79B9) ^ (n << 20)) & ((1 << 53) - 1);
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, tx);
        (id, rx)
    }

    fn forget(&self, id: u64) {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }
}

/// On window open: decides in the background whether the Fast Model drives it.
pub fn check_vision(app: &AppHandle, label: &str) {
    let Some(agents) = app.try_state::<Agents>() else {
        return;
    };
    agents.set_mode(label, Mode::Pending);
    let (app, label) = (app.clone(), label.to_string());
    tauri::async_runtime::spawn(async move {
        let st = app.state::<AppState>();
        let mode = match (st.active_model().await, active_engine(&st).await) {
            (Ok(model), Some(engine)) if supports_images(&st, &model, engine.as_ref()).await => {
                Mode::Vision
            }
            _ => Mode::Dom,
        };
        app.state::<Agents>().set_mode(&label, mode);
    });
}

/// The top frame settled. Starts the agent if this window uses it.
pub fn on_ready(window: &WebviewWindow) {
    let label = window.label().to_string();
    let Some(agents) = window.try_state::<Agents>() else {
        return;
    };
    let Some(stop) = agents.begin(&label) else {
        return;
    };
    let window = window.clone();
    tauri::async_runtime::spawn(async move {
        let inner = window.clone();
        let stop_inner = stop.clone();
        // A panic inside the run still unlocks the window below.
        let end = tauri::async_runtime::spawn(async move { drive(&inner, stop_inner).await })
            .await
            .unwrap_or_else(|e| End::Failed(format!("internal error: {e}")));
        let page = WindowPage::new(&window, stop);
        page.lock(false).await;
        window.state::<Agents>().finish(&label);
        if end == End::NoVision {
            let st = window.state::<AppState>();
            if let Ok(model) = st.active_model().await {
                let _ = st.store.set_setting(&cache_key(&model), "false").await;
            }
        }
        let _ = notice(&window, &end.notice());
        if end.falls_back() {
            let _ = window.eval("window.__cvdAutofill && window.__cvdAutofill.rescan()");
        }
    });
}

async fn drive(window: &WebviewWindow, stop: Arc<AtomicBool>) -> End {
    let st = window.state::<AppState>();
    let sessions = window.state::<Sessions>();
    let label = window.label().to_string();
    let Some(top) = window.url().ok().and_then(|u| https_host(u.as_str())) else {
        return End::Failed("Autofill only works on HTTPS pages".into());
    };
    if !consent(window, &sessions, &label, &[top]).await {
        return End::UserStopped;
    }
    let Some(engine) = active_engine(&st).await else {
        return End::Failed("Choose a Fast Model in AI Models".into());
    };
    let facts = match crate::commands::apply_facts(&st).await {
        Ok(f) => f,
        Err(e) => return End::Failed(e.to_string()),
    };
    let names: Vec<&str> = facts.iter().map(|f| f.key).collect();
    let page = WindowPage::new(window, stop);
    run(&page, engine.as_ref(), &names).await
}

struct WindowPage {
    window: WebviewWindow,
    stop: Arc<AtomicBool>,
}

impl WindowPage {
    fn new(window: &WebviewWindow, stop: Arc<AtomicBool>) -> Self {
        Self {
            window: window.clone(),
            stop,
        }
    }

    fn eval(&self, cmd: Value) -> Result<(), String> {
        self.window
            .eval(format!(
                "window.__cvdAutofill && window.__cvdAutofill.agent({cmd})"
            ))
            .map_err(|e| e.to_string())
    }

    /// Sends `cmd` and waits for the page's `agent_reply`.
    async fn request(&self, mut cmd: Value, wait: Duration) -> Option<Value> {
        let agents = self.window.state::<Agents>();
        let (id, rx) = agents.expect();
        cmd["id"] = json!(id);
        if self.eval(cmd).is_err() {
            agents.forget(id);
            return None;
        }
        let got = tokio::time::timeout(wait, rx)
            .await
            .ok()
            .and_then(Result::ok);
        if got.is_none() {
            agents.forget(id);
        }
        got
    }
}

/// Marks come from a remote page: cap and clean them like fields.
fn clean(mut screen: Screen) -> Screen {
    let cap = |s: &mut String, n: usize| {
        if s.len() > n {
            let mut end = n;
            while !s.is_char_boundary(end) {
                end -= 1;
            }
            s.truncate(end);
        }
    };
    screen.marks.truncate(MAX_MARKS);
    cap(&mut screen.url, 2000);
    cap(&mut screen.title, 200);
    for m in &mut screen.marks {
        cap(&mut m.label, 200);
        m.field = m.field.take().map(FieldSpec::sanitized);
    }
    screen
}

#[async_trait]
impl Page for WindowPage {
    async fn lock(&self, on: bool) {
        let _ = self.eval(json!({ "op": "lock", "on": on }));
    }

    async fn screen(&self) -> Result<Screen, String> {
        let reply = self
            .request(json!({ "op": "marks" }), MARKS_TIMEOUT)
            .await
            .ok_or("The page stopped answering")?;
        serde_json::from_value(reply)
            .map(clean)
            .map_err(|e| format!("The page sent bad marks: {e}"))
    }

    async fn screenshot(&self) -> Result<String, String> {
        let window = self.window.clone();
        tokio::task::spawn_blocking(move || capture::capture(&window))
            .await
            .map_err(|e| e.to_string())?
    }

    async fn act(&self, action: &Action, mark: Option<&Mark>) -> Acted {
        let what = match action {
            Action::Click(_) => "click",
            Action::ScrollDown => "scroll_down",
            Action::ScrollUp => "scroll_up",
            Action::Back => "back",
            _ => return Acted::Missing,
        };
        let cmd = json!({ "op": "act", "do": what, "n": mark.map(|m| m.n) });
        match self.request(cmd, ACT_TIMEOUT).await {
            Some(r) => match r.get("result").and_then(Value::as_str) {
                Some("submit_blocked") => Acted::SubmitBlocked,
                Some("missing") => Acted::Missing,
                _ => Acted::Ok,
            },
            None => Acted::Ok,
        }
    }

    async fn fill(&self, mark: &Mark) -> Filled {
        let Some(field) = mark.field.clone() else {
            return Filled::NeedsUser;
        };
        let (st, sessions) = (
            self.window.state::<AppState>(),
            self.window.state::<Sessions>(),
        );
        let label = self.window.label().to_string();
        let Some(job_id) = job_id_of(&label) else {
            return Filled::NeedsUser;
        };
        let mut hosts: Vec<String> = self
            .window
            .url()
            .ok()
            .and_then(|u| https_host(u.as_str()))
            .into_iter()
            .collect();
        if let Some(frame) = mark.origin.as_deref().and_then(https_host) {
            if !hosts.contains(&frame) {
                hosts.push(frame);
            }
        }
        if hosts.is_empty() || !consent(&self.window, &sessions, &label, &hosts).await {
            return Filled::NeedsUser;
        }
        match fill_fields(&self.window, &st, &sessions, job_id, vec![field]).await {
            Ok(actions)
                if actions
                    .iter()
                    .any(|a| !matches!(a, FillAction::NeedsUser { .. })) =>
            {
                Filled::Filled
            }
            _ => Filled::NeedsUser,
        }
    }

    async fn needs_user(&self, mark: &Mark, reason: &str) {
        if let Some(f) = &mark.field {
            let _ = send(
                &self.window,
                &[FillAction::NeedsUser {
                    key: f.key.clone(),
                    reason: reason.into(),
                }],
            );
        }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst) || self.window.is_visible().is_err()
    }

    #[cfg(debug_assertions)]
    fn dump(&self, step: usize, state: &str, screenshot: &str, answer: &str) {
        use base64::Engine as _;
        let Ok(dir) = std::env::var("CVD_AGENT_DUMP") else {
            return;
        };
        let stamp = chrono::Local::now().format("%H%M%S");
        let base = format!("{dir}/{stamp}-{step:02}");
        if let Ok(png) = base64::engine::general_purpose::STANDARD.decode(screenshot) {
            let _ = std::fs::write(format!("{base}.png"), png);
        }
        let _ = std::fs::write(
            format!("{base}.json"),
            serde_json::to_string_pretty(&json!({ "state": state, "answer": answer }))
                .unwrap_or_default(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unknown_windows_are_dom_only() {
        assert_eq!(Agents::default().mode("apply-1").await, Mode::Dom);
    }

    #[tokio::test]
    async fn fields_wait_for_the_vision_check() {
        let agents = Arc::new(Agents::default());
        agents.set_mode("apply-1", Mode::Pending);
        let waiter = {
            let a = agents.clone();
            tokio::spawn(async move { a.mode("apply-1").await })
        };
        tokio::task::yield_now().await;
        agents.set_mode("apply-1", Mode::Vision);
        assert_eq!(waiter.await.unwrap(), Mode::Vision);
    }

    #[test]
    fn one_run_per_window_then_dom() {
        let agents = Agents::default();
        assert!(agents.begin("apply-1").is_none());
        agents.set_mode("apply-1", Mode::Vision);
        let stop = agents.begin("apply-1").unwrap();
        assert!(agents.begin("apply-1").is_none());
        agents.stop("apply-1");
        assert!(stop.load(Ordering::SeqCst));
        agents.finish("apply-1");
        assert!(agents.begin("apply-1").is_none());
    }

    #[tokio::test]
    async fn replies_reach_their_request_once() {
        let agents = Agents::default();
        let (id, rx) = agents.expect();
        assert!(id < 1 << 53);
        agents.reply(id + 1, json!("stray"));
        agents.reply(id, json!({"ok": true}));
        agents.reply(id, json!("again"));
        assert_eq!(rx.await.unwrap(), json!({"ok": true}));
    }

    #[test]
    fn marks_are_capped_and_sanitized() {
        let long = "x".repeat(5000);
        let screen: Screen = serde_json::from_value(json!({
            "url": long, "title": long,
            "marks": (0..400).map(|n| json!({"n": n, "kind": "field", "label": long,
                "field": {"key": "k", "label": long}})).collect::<Vec<_>>()
        }))
        .unwrap();
        let s = clean(screen);
        assert_eq!(s.marks.len(), MAX_MARKS);
        assert!(s.title.len() <= 200 && s.marks[0].label.len() <= 200);
        assert!(s.marks[0].field.as_ref().unwrap().label.len() <= 500);
    }
}
