//! One LISTEN connection fanned out to per-tenant subscribers.
use super::super::{Result, UserId};
use sqlx::postgres::PgListener;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

const CHANNEL: &str = "db_changed";
/// Same cadence as the SQLite poller, so bursts reach the UI as one event.
const DEBOUNCE: Duration = Duration::from_millis(250);
const RETRY: Duration = Duration::from_secs(1);
/// Tables with change triggers; sent in full after a reconnect may have lost notifications.
pub const TABLES: &[&str] = &[
    "profile",
    "entry",
    "bullet",
    "skill",
    "entry_skill",
    "job_description",
    "match_run",
    "match_score",
    "cv",
    "settings",
    "secrets",
    "job_status",
    "company",
    "company_alias",
    "scan_run",
    "processed_email",
    "apply_memory",
    "job_view",
];

/// Changed table names for one tenant, debounced.
pub type ChangeReceiver = mpsc::UnboundedReceiver<Vec<String>>;
type Subscribers = Arc<Mutex<HashMap<UserId, Vec<mpsc::UnboundedSender<Vec<String>>>>>>;

enum Signal {
    Changed(UserId, String),
    /// Notifications may have been missed; every subscriber should refresh.
    Reconnected,
}

#[derive(serde::Deserialize)]
struct Payload {
    user_id: String,
    table: String,
}

/// Stops listening when dropped.
pub struct ChangeFeed {
    subscribers: Subscribers,
    tasks: [JoinHandle<()>; 2],
}
impl ChangeFeed {
    /// Needs a direct or session-mode connection: transaction poolers drop LISTEN.
    pub async fn connect(url: &str) -> Result<Self> {
        let listener = listen(url).await?;
        let subscribers = Subscribers::default();
        let (tx, rx) = mpsc::unbounded_channel();
        let reader = tokio::spawn(read(url.to_owned(), listener, tx));
        let fanout = tokio::spawn(fan_out(rx, subscribers.clone()));
        Ok(Self {
            subscribers,
            tasks: [reader, fanout],
        })
    }
    /// Receives only this tenant's changes. Drop the receiver to unsubscribe.
    pub fn subscribe(&self, user: UserId) -> ChangeReceiver {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut subscribers = self.subscribers.lock().unwrap();
        // Users the server unloaded leave closed senders behind; drop them all here.
        subscribers.retain(|_, senders| {
            senders.retain(|s| !s.is_closed());
            !senders.is_empty()
        });
        subscribers.entry(user).or_default().push(tx);
        rx
    }
    #[cfg(test)]
    pub fn subscriber_count(&self, user: UserId) -> usize {
        let subscribers = self.subscribers.lock().unwrap();
        subscribers
            .get(&user)
            .map_or(0, |s| s.iter().filter(|s| !s.is_closed()).count())
    }
}
impl Drop for ChangeFeed {
    fn drop(&mut self) {
        self.tasks.iter().for_each(JoinHandle::abort);
    }
}

async fn listen(url: &str) -> Result<PgListener> {
    let mut listener = PgListener::connect(url).await?;
    listener.listen(CHANNEL).await?;
    Ok(listener)
}

/// Owns the connection; never cancelled mid-message.
async fn read(url: String, mut listener: PgListener, tx: mpsc::UnboundedSender<Signal>) {
    loop {
        let signal = match listener.try_recv().await {
            Ok(Some(n)) => match parse(n.payload()) {
                Some((user, table)) => Signal::Changed(user, table),
                None => continue,
            },
            // sqlx has already reconnected and re-subscribed.
            Ok(None) => Signal::Reconnected,
            Err(_) => loop {
                tokio::time::sleep(RETRY).await;
                if let Ok(fresh) = listen(&url).await {
                    listener = fresh;
                    break Signal::Reconnected;
                }
            },
        };
        if tx.send(signal).is_err() {
            return;
        }
    }
}

fn parse(payload: &str) -> Option<(UserId, String)> {
    let p: Payload = serde_json::from_str(payload).ok()?;
    Some((UserId(p.user_id.parse().ok()?), p.table))
}

async fn fan_out(mut rx: mpsc::UnboundedReceiver<Signal>, subscribers: Subscribers) {
    while let Some(first) = rx.recv().await {
        let mut pending: HashMap<UserId, BTreeSet<String>> = HashMap::new();
        let mut everyone = false;
        let mut apply = |signal| match signal {
            Signal::Changed(user, table) => {
                pending.entry(user).or_default().insert(table);
            }
            Signal::Reconnected => everyone = true,
        };
        apply(first);
        let deadline = tokio::time::sleep(DEBOUNCE);
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                Some(signal) = rx.recv() => apply(signal),
                _ = &mut deadline => break,
            }
        }
        let mut subscribers = subscribers.lock().unwrap();
        subscribers.retain(|user, senders| {
            let tables: Vec<String> = if everyone {
                TABLES.iter().map(|t| t.to_string()).collect()
            } else if let Some(tables) = pending.get(user) {
                tables.iter().cloned().collect()
            } else {
                return true;
            };
            senders.retain(|s| s.send(tables.clone()).is_ok());
            !senders.is_empty()
        });
    }
}
