//! Watches the scan folder over IMAP IDLE while the app is open and runs a
//! scan cycle whenever new mail arrives.

use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::Notify;
use tokio::task::AbortHandle;

use super::cycle::CycleError;
use super::imap::MailError;
use crate::services::AppError;
use crate::state::Emitter;

pub const WATCHER_EVENT: &str = "mail://watcher";
/// Gmail drops idle connections before RFC 2177's 29 minutes, so IDLE is
/// re-issued well inside that.
pub const REIDLE: Duration = Duration::from_secs(10 * 60);
pub const BACKOFF_MIN: Duration = Duration::from_secs(5);
pub const BACKOFF_MAX: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
pub enum WatchState {
    Connecting,
    Watching,
    Scanning,
    Paused,
    #[default]
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, ts_rs::TS)]
pub struct WatcherStatus {
    pub state: WatchState,
    pub error: Option<String>,
    #[ts(type = "AppErrorKind | null")]
    pub error_kind: Option<String>,
    /// RFC 3339 time of the last successful cycle.
    pub last_checked_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Reconnect with fresh settings (and leave `paused` or `stopped`).
    Restart,
    Stop,
    /// Scan now, e.g. after a job was added.
    Nudge,
    /// Leave `paused`.
    Retry,
}

/// The app's handle on the watcher task: send it controls, read its status.
pub struct WatcherControl {
    tx: UnboundedSender<Control>,
    rx: Mutex<Option<UnboundedReceiver<Control>>>,
    status: Arc<Mutex<WatcherStatus>>,
    /// The running task, so `shutdown` can end it; it holds the state alive otherwise.
    task: Mutex<Option<AbortHandle>>,
}

impl Default for WatcherControl {
    fn default() -> Self {
        let (tx, rx) = unbounded_channel();
        Self {
            tx,
            rx: Mutex::new(Some(rx)),
            status: Arc::default(),
            task: Mutex::default(),
        }
    }
}

impl WatcherControl {
    /// Queues a control; a no-op when no watcher task runs (tests).
    pub fn send(&self, control: Control) {
        let _ = self.tx.send(control);
    }

    pub fn status(&self) -> WatcherStatus {
        lock(&self.status).clone()
    }

    pub fn status_handle(&self) -> Arc<Mutex<WatcherStatus>> {
        self.status.clone()
    }

    /// The receiving end, handed to `run_watcher` once.
    pub fn take_receiver(&self) -> Option<UnboundedReceiver<Control>> {
        lock(&self.rx).take()
    }

    /// Records the task running `run_watcher`.
    pub fn attach(&self, task: AbortHandle) {
        *lock(&self.task) = Some(task);
    }

    /// Ends the watcher task for good, e.g. when the server unloads an idle user.
    pub fn shutdown(&self) {
        if let Some(task) = lock(&self.task).take() {
            task.abort();
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdleEvent {
    NewData,
    Timeout,
}

/// One IDLE connection. `wait` returns `Timeout` after at most `REIDLE`.
#[async_trait]
pub trait IdleConn: Send {
    async fn wait(&mut self) -> Result<IdleEvent, MailError>;
}

#[async_trait]
pub trait WatchDeps: Send + Sync {
    /// `Ok(None)` when no Gmail account is saved.
    async fn connect(&self) -> Result<Option<Box<dyn IdleConn>>, MailError>;
    async fn cycle(&self) -> Result<(), CycleError>;
    fn scan_freed(&self) -> Arc<Notify>;
}

/// Resolves to `None` once `REIDLE` of wall-clock time passes. Unlike
/// async-imap's own timeout, keepalives do not extend it, so a dead socket
/// is noticed.
pub async fn reidle_deadline<F: Future>(fut: F) -> Option<F::Output> {
    tokio::time::timeout(REIDLE, fut).await.ok()
}

/// Errors that need the user, so retrying on a timer would only repeat them.
fn is_fatal(e: &AppError) -> bool {
    matches!(e.kind(), "MailAuth" | "ProviderAuth" | "ProviderCredits") || needs_setup(e.kind())
}

/// Setup the app is missing (no job or status yet, no model chosen). Adding a
/// job, which nudges the watcher, may fix it.
fn needs_setup(kind: &str) -> bool {
    matches!(kind, "Io" | "ModelMissing" | "NotFound")
}

/// Where the loop goes next.
enum Flow {
    Connect,
    Backoff,
    Stop,
    Pause(String, String),
    Exit,
}

/// How a run of cycles ended.
enum Cycles {
    Done,
    Then(Flow),
}

struct Watcher {
    deps: Arc<dyn WatchDeps>,
    rx: UnboundedReceiver<Control>,
    status: Arc<Mutex<WatcherStatus>>,
    emit: Emitter,
    backoff: Duration,
}

pub async fn run_watcher(
    deps: Arc<dyn WatchDeps>,
    rx: UnboundedReceiver<Control>,
    status: Arc<Mutex<WatcherStatus>>,
    emit: Emitter,
) {
    let mut w = Watcher {
        deps,
        rx,
        status,
        emit,
        backoff: BACKOFF_MIN,
    };
    let mut flow = Flow::Stop;
    loop {
        flow = match flow {
            Flow::Exit => return,
            Flow::Stop => w.stopped().await,
            Flow::Pause(error, kind) => w.paused(error, kind).await,
            Flow::Backoff => w.back_off().await,
            Flow::Connect => w.connect().await,
        };
    }
}

impl Watcher {
    fn set(&self, state: WatchState) {
        let mut status = lock(&self.status);
        status.state = state;
        if state != WatchState::Paused {
            status.error = None;
            status.error_kind = None;
        }
        let payload = serde_json::to_value(&*status).unwrap_or_default();
        drop(status);
        (self.emit)(WATCHER_EVENT, payload);
    }

    async fn stopped(&mut self) -> Flow {
        self.set(WatchState::Stopped);
        loop {
            match self.rx.recv().await {
                None => return Flow::Exit,
                Some(Control::Restart) => return self.restart(),
                Some(_) => {}
            }
        }
    }

    async fn paused(&mut self, error: String, kind: String) -> Flow {
        let wakes_on_nudge = needs_setup(&kind);
        {
            let mut status = lock(&self.status);
            status.error = Some(error);
            status.error_kind = Some(kind);
        }
        self.set(WatchState::Paused);
        loop {
            match self.rx.recv().await {
                None => return Flow::Exit,
                Some(Control::Restart | Control::Retry) => return self.restart(),
                Some(Control::Stop) => return Flow::Stop,
                Some(Control::Nudge) if wakes_on_nudge => return self.restart(),
                Some(Control::Nudge) => {}
            }
        }
    }

    fn restart(&mut self) -> Flow {
        self.backoff = BACKOFF_MIN;
        Flow::Connect
    }

    async fn back_off(&mut self) -> Flow {
        self.set(WatchState::Connecting);
        let delay = self.backoff;
        self.backoff = (self.backoff * 2).min(BACKOFF_MAX);
        let sleep = tokio::time::sleep(delay);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                _ = &mut sleep => return Flow::Connect,
                control = self.rx.recv() => match control {
                    None => return Flow::Exit,
                    Some(Control::Stop) => return Flow::Stop,
                    Some(Control::Restart | Control::Retry) => return self.restart(),
                    Some(Control::Nudge) => {}
                },
            }
        }
    }

    async fn connect(&mut self) -> Flow {
        self.set(WatchState::Connecting);
        let deps = self.deps.clone();
        let connecting = deps.connect();
        tokio::pin!(connecting);
        let result = loop {
            tokio::select! {
                result = &mut connecting => break result,
                control = self.rx.recv() => match control {
                    None => return Flow::Exit,
                    Some(Control::Stop) => return Flow::Stop,
                    Some(Control::Restart) => return self.restart(),
                    Some(Control::Nudge | Control::Retry) => {}
                },
            }
        };
        match result {
            // The backoff resets only after a cycle succeeds, so a server
            // that accepts logins but fails every scan is not retried every 5 s.
            Ok(Some(conn)) => self.watch(conn).await,
            Ok(None) => Flow::Stop,
            Err(e) => {
                let e = AppError::from(e);
                if is_fatal(&e) {
                    Flow::Pause(e.to_string(), e.kind().to_string())
                } else {
                    Flow::Backoff
                }
            }
        }
    }

    /// Catches up, then scans on each new-mail signal until the connection
    /// fails or a control moves the watcher elsewhere.
    async fn watch(&mut self, mut conn: Box<dyn IdleConn>) -> Flow {
        let mut scan = true;
        loop {
            if scan {
                if let Cycles::Then(flow) = self.cycles().await {
                    return flow;
                }
                scan = false;
            }
            self.set(WatchState::Watching);
            enum Woke {
                Idle(Result<IdleEvent, MailError>),
                Control(Option<Control>),
            }
            let woke = tokio::select! {
                event = conn.wait() => Woke::Idle(event),
                control = self.rx.recv() => Woke::Control(control),
            };
            match woke {
                Woke::Idle(Ok(IdleEvent::NewData)) => scan = true,
                Woke::Idle(Ok(IdleEvent::Timeout)) => {}
                Woke::Idle(Err(_)) => return Flow::Backoff,
                Woke::Control(None) => return Flow::Exit,
                Woke::Control(Some(Control::Stop)) => return Flow::Stop,
                Woke::Control(Some(Control::Restart)) => return self.restart(),
                Woke::Control(Some(Control::Retry)) => {}
                // Leaving IDLE mid-command spoils the session, so reconnect;
                // the catch-up cycle after connecting is the requested scan.
                Woke::Control(Some(Control::Nudge)) => return Flow::Connect,
            }
        }
    }

    /// Runs one cycle, plus one more if nudged meanwhile. Waits for the slot
    /// when another scan holds it.
    async fn cycles(&mut self) -> Cycles {
        let mut dirty = false;
        loop {
            self.set(WatchState::Scanning);
            let freed = self.deps.scan_freed();
            let notified = freed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();

            let deps = self.deps.clone();
            let cycle = deps.cycle();
            tokio::pin!(cycle);
            let mut after: Option<Flow> = None;
            let result = loop {
                tokio::select! {
                    result = &mut cycle => break result,
                    control = self.rx.recv() => match control {
                        None => after = Some(Flow::Exit),
                        Some(Control::Nudge) => dirty = true,
                        Some(Control::Stop) => after = Some(Flow::Stop),
                        Some(Control::Restart) => after = Some(Flow::Connect),
                        Some(Control::Retry) => {}
                    },
                }
            };
            if let Some(flow) = after {
                if matches!(flow, Flow::Connect) {
                    self.backoff = BACKOFF_MIN;
                }
                return Cycles::Then(flow);
            }
            match result {
                Ok(()) => {
                    self.backoff = BACKOFF_MIN;
                    lock(&self.status).last_checked_at = Some(chrono::Utc::now().to_rfc3339());
                    if dirty {
                        dirty = false;
                        continue;
                    }
                    return Cycles::Done;
                }
                Err(CycleError::Busy) => loop {
                    tokio::select! {
                        _ = &mut notified => break,
                        control = self.rx.recv() => match control {
                            None => return Cycles::Then(Flow::Exit),
                            Some(Control::Stop) => return Cycles::Then(Flow::Stop),
                            Some(Control::Restart) => return Cycles::Then(self.restart()),
                            Some(Control::Nudge | Control::Retry) => {}
                        },
                    }
                },
                Err(CycleError::NoAccount) => return Cycles::Then(Flow::Stop),
                Err(CycleError::Failed(e)) if is_fatal(&e) => {
                    return Cycles::Then(Flow::Pause(e.to_string(), e.kind().to_string()))
                }
                Err(CycleError::Failed(_)) => return Cycles::Then(Flow::Backoff),
            }
        }
    }
}
