use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::Notify;

use super::cycle::CycleError;
use super::imap::MailError;
use super::watch::*;
use crate::commands::AppError;
use crate::state::Emitter;

type Script<T> = Arc<Mutex<VecDeque<T>>>;

/// Pops scripted IDLE events; waits forever once the script runs out.
struct FakeConn(Script<Result<IdleEvent, MailError>>);

#[async_trait]
impl IdleConn for FakeConn {
    async fn wait(&mut self) -> Result<IdleEvent, MailError> {
        let next = self.0.lock().unwrap().pop_front();
        match next {
            Some(event) => event,
            None => std::future::pending().await,
        }
    }
}

#[derive(Default)]
struct FakeDeps {
    /// `None` = no account; missing entries connect successfully.
    connects: Script<Result<bool, MailError>>,
    idle: Script<Result<IdleEvent, MailError>>,
    cycles_script: Script<Result<(), CycleError>>,
    cycle_delay: Mutex<Duration>,
    connects_made: AtomicUsize,
    cycles: AtomicUsize,
    freed: Arc<Notify>,
}

#[async_trait]
impl WatchDeps for FakeDeps {
    async fn connect(&self) -> Result<Option<Box<dyn IdleConn>>, MailError> {
        self.connects_made.fetch_add(1, Ordering::SeqCst);
        let next = self.connects.lock().unwrap().pop_front();
        match next.unwrap_or(Ok(true)) {
            Ok(true) => Ok(Some(Box::new(FakeConn(self.idle.clone())))),
            Ok(false) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn cycle(&self) -> Result<(), CycleError> {
        self.cycles.fetch_add(1, Ordering::SeqCst);
        let delay = *self.cycle_delay.lock().unwrap();
        tokio::time::sleep(delay).await;
        let next = self.cycles_script.lock().unwrap().pop_front();
        next.unwrap_or(Ok(()))
    }

    fn scan_freed(&self) -> Arc<Notify> {
        self.freed.clone()
    }
}

struct Harness {
    deps: Arc<FakeDeps>,
    tx: UnboundedSender<Control>,
    states: Arc<Mutex<Vec<WatcherStatus>>>,
}

impl Harness {
    fn cycles(&self) -> usize {
        self.deps.cycles.load(Ordering::SeqCst)
    }

    fn connects(&self) -> usize {
        self.deps.connects_made.load(Ordering::SeqCst)
    }

    fn last(&self) -> WatcherStatus {
        self.states.lock().unwrap().last().cloned().unwrap()
    }
}

/// Starts the watcher on fake deps and sends the initial `Restart`.
fn start(deps: FakeDeps) -> Harness {
    let deps = Arc::new(deps);
    let (tx, rx) = unbounded_channel();
    let states: Arc<Mutex<Vec<WatcherStatus>>> = Arc::default();
    let sink = states.clone();
    let emit: Emitter = Arc::new(move |name: &str, payload: serde_json::Value| {
        assert_eq!(name, WATCHER_EVENT);
        sink.lock()
            .unwrap()
            .push(serde_json::from_value(payload).unwrap());
    });
    let status = Arc::new(Mutex::new(WatcherStatus::default()));
    tokio::spawn(run_watcher(deps.clone(), rx, status, emit));
    tx.send(Control::Restart).unwrap();
    Harness { deps, tx, states }
}

async fn after(secs: u64) {
    tokio::time::sleep(Duration::from_secs(secs)).await;
}

fn script<T>(items: Vec<T>) -> Script<T> {
    Arc::new(Mutex::new(items.into()))
}

#[tokio::test(start_paused = true)]
async fn connect_runs_catch_up_cycle() {
    let h = start(FakeDeps::default());
    after(1).await;

    assert_eq!(h.cycles(), 1);
    assert_eq!(h.last().state, WatchState::Watching);
    assert!(h.last().last_checked_at.is_some());
}

#[tokio::test(start_paused = true)]
async fn new_data_runs_cycle() {
    let h = start(FakeDeps {
        idle: script(vec![Ok(IdleEvent::NewData)]),
        ..FakeDeps::default()
    });
    after(1).await;

    assert_eq!(h.cycles(), 2);
    assert_eq!(h.connects(), 1, "NewData keeps the connection");
}

#[tokio::test(start_paused = true)]
async fn idle_timeout_waits_again_without_scanning() {
    let h = start(FakeDeps {
        idle: script(vec![Ok(IdleEvent::Timeout), Ok(IdleEvent::Timeout)]),
        ..FakeDeps::default()
    });
    after(1).await;

    assert_eq!(h.cycles(), 1);
    assert_eq!(h.connects(), 1);
}

#[tokio::test(start_paused = true)]
async fn signals_during_scan_coalesce_into_one_follow_up() {
    let deps = FakeDeps::default();
    *deps.cycle_delay.lock().unwrap() = Duration::from_secs(30);
    let h = start(deps);
    after(5).await;
    for _ in 0..3 {
        h.tx.send(Control::Nudge).unwrap();
    }
    after(120).await;

    assert_eq!(h.cycles(), 2);
}

#[tokio::test(start_paused = true)]
async fn nudge_while_watching_scans_and_reconnects() {
    let h = start(FakeDeps::default());
    after(1).await;
    h.tx.send(Control::Nudge).unwrap();
    after(1).await;

    assert_eq!(h.cycles(), 2);
    assert_eq!(h.connects(), 2);
}

#[tokio::test(start_paused = true)]
async fn connection_error_backs_off() {
    let h = start(FakeDeps {
        connects: script(vec![
            Err(MailError::Connection("down".into())),
            Err(MailError::Connection("down".into())),
        ]),
        ..FakeDeps::default()
    });
    after(14).await;
    assert_eq!(h.cycles(), 0, "5 s then 10 s of backoff");
    assert_eq!(h.connects(), 2);

    after(2).await;
    assert_eq!(h.cycles(), 1);
}

#[tokio::test(start_paused = true)]
async fn backoff_caps_at_five_minutes() {
    let errors = (0..10)
        .map(|_| Err(MailError::Connection("down".into())))
        .collect();
    let h = start(FakeDeps {
        connects: script(errors),
        ..FakeDeps::default()
    });
    // 5 + 10 + 20 + 40 + 80 + 160 = 315 s for six failures, then 300 s each.
    after(316).await;
    assert_eq!(h.connects(), 7);
    after(298).await;
    assert_eq!(h.connects(), 7);
    after(2).await;
    assert_eq!(h.connects(), 8);
}

#[tokio::test(start_paused = true)]
async fn successful_connect_resets_backoff() {
    let h = start(FakeDeps {
        connects: script(vec![
            Err(MailError::Connection("down".into())),
            Err(MailError::Connection("down".into())),
            Ok(true),
            Err(MailError::Connection("down".into())),
        ]),
        idle: script(vec![Err(MailError::Connection("dropped".into()))]),
        ..FakeDeps::default()
    });
    // Fails at 0 and 5, connects at 15, IDLE drops, reconnects at 20 (5 s:
    // reset, not 20 s) and fails, then succeeds at 30.
    after(16).await;
    assert_eq!(h.cycles(), 1);
    after(5).await;
    assert_eq!(h.connects(), 4);
    after(10).await;
    assert_eq!(h.connects(), 5);
    assert_eq!(h.cycles(), 2);
}

#[tokio::test(start_paused = true)]
async fn auth_failure_pauses_until_retry() {
    let h = start(FakeDeps {
        connects: script(vec![Err(MailError::Auth("bad password".into()))]),
        ..FakeDeps::default()
    });
    after(1).await;
    assert_eq!(h.last().state, WatchState::Paused);
    assert_eq!(h.last().error_kind.as_deref(), Some("MailAuth"));

    after(3600).await;
    assert_eq!(h.connects(), 1, "no retries while paused");

    h.tx.send(Control::Retry).unwrap();
    after(1).await;
    assert_eq!(h.cycles(), 1);
    assert_eq!(h.last().state, WatchState::Watching);
    assert_eq!(h.last().error, None);
}

#[tokio::test(start_paused = true)]
async fn nudge_while_paused_does_nothing() {
    let h = start(FakeDeps {
        connects: script(vec![Err(MailError::Auth("bad password".into()))]),
        ..FakeDeps::default()
    });
    after(1).await;
    h.tx.send(Control::Nudge).unwrap();
    after(1).await;

    assert_eq!(h.connects(), 1);
    assert_eq!(h.last().state, WatchState::Paused);
}

#[tokio::test(start_paused = true)]
async fn fatal_cycle_error_pauses() {
    let h = start(FakeDeps {
        cycles_script: script(vec![Err(CycleError::Failed(AppError::ProviderCredits(
            "out of credits".into(),
        )))]),
        ..FakeDeps::default()
    });
    after(600).await;

    assert_eq!(h.last().state, WatchState::Paused);
    assert_eq!(h.last().error_kind.as_deref(), Some("ProviderCredits"));
    assert_eq!(h.cycles(), 1);
}

#[tokio::test(start_paused = true)]
async fn transient_cycle_error_backs_off() {
    let h = start(FakeDeps {
        cycles_script: script(vec![Err(CycleError::Failed(
            AppError::ProviderUnavailable("timeout".into()),
        ))]),
        ..FakeDeps::default()
    });
    after(4).await;
    assert_eq!(h.cycles(), 1);
    assert_eq!(h.connects(), 1);

    after(2).await;
    assert_eq!(h.connects(), 2);
    assert_eq!(h.cycles(), 2);
}

#[tokio::test(start_paused = true)]
async fn busy_waits_for_slot_then_scans() {
    let h = start(FakeDeps {
        cycles_script: script(vec![Err(CycleError::Busy)]),
        ..FakeDeps::default()
    });
    after(60).await;
    assert_eq!(h.cycles(), 1);

    h.deps.freed.notify_waiters();
    after(1).await;
    assert_eq!(h.cycles(), 2);
    assert_eq!(h.last().state, WatchState::Watching);
}

#[tokio::test(start_paused = true)]
async fn no_account_stops_and_restart_resumes() {
    let h = start(FakeDeps {
        connects: script(vec![Ok(false)]),
        ..FakeDeps::default()
    });
    after(60).await;
    assert_eq!(h.last().state, WatchState::Stopped);
    assert_eq!(h.connects(), 1);

    h.tx.send(Control::Restart).unwrap();
    after(1).await;
    assert_eq!(h.cycles(), 1);
}

#[tokio::test(start_paused = true)]
async fn stop_drops_the_connection() {
    let h = start(FakeDeps::default());
    after(1).await;
    h.tx.send(Control::Stop).unwrap();
    after(1).await;

    assert_eq!(h.last().state, WatchState::Stopped);
    after(3600).await;
    assert_eq!(h.connects(), 1);
}

#[tokio::test(start_paused = true)]
async fn idle_that_never_returns_is_reissued_after_ten_minutes() {
    let start = tokio::time::Instant::now();

    let result = reidle_deadline(std::future::pending::<()>()).await;

    assert_eq!(result, None);
    assert_eq!(start.elapsed(), REIDLE);
    assert_eq!(REIDLE, Duration::from_secs(600));
}

#[tokio::test(start_paused = true)]
async fn repeated_cycle_failures_grow_backoff_despite_reconnects() {
    let unavailable = || {
        Err(CycleError::Failed(AppError::ProviderUnavailable(
            "down".into(),
        )))
    };
    let h = start(FakeDeps {
        cycles_script: script(vec![unavailable(), unavailable(), unavailable()]),
        ..FakeDeps::default()
    });
    // Fails at 0, 5 and 15; succeeds at 35.
    after(14).await;
    assert_eq!(h.cycles(), 2);
    after(2).await;
    assert_eq!(h.cycles(), 3);
    after(18).await;
    assert_eq!(h.cycles(), 3);
    after(2).await;
    assert_eq!(h.cycles(), 4);
    assert_eq!(h.last().state, WatchState::Watching);
}

#[tokio::test(start_paused = true)]
async fn missing_setup_pauses_until_a_job_is_added() {
    let h = start(FakeDeps {
        cycles_script: script(vec![Err(CycleError::Failed(AppError::Io(
            "Add a job and a status first.".into(),
        )))]),
        ..FakeDeps::default()
    });
    after(3600).await;
    assert_eq!(h.last().state, WatchState::Paused);
    assert_eq!(h.connects(), 1);
    assert_eq!(h.cycles(), 1);

    h.tx.send(Control::Nudge).unwrap();
    after(1).await;
    assert_eq!(h.cycles(), 2);
    assert_eq!(h.last().state, WatchState::Watching);
}
