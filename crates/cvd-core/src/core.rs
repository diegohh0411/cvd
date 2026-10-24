//! Runtime-independent application startup. Call from an active Tokio runtime.

use std::{path::Path, sync::Arc, time::Duration};

use crate::mail::{cycle::AppWatchDeps, watch};
use crate::services::AppError;
use crate::state::{AppState, Emitter, EngineSource, DB_CHANGED_EVENT};
use crate::store::{ChangeReceiver, TenantStore as Store};

/// Opens the application's store, restores providers and starts background services.
/// The caller owns the runtime and keeps it alive for the application's lifetime.
pub async fn bootstrap(data_dir: &Path, emit: Emitter) -> Result<Arc<AppState>, AppError> {
    std::fs::create_dir_all(data_dir)?;
    let url = format!("sqlite://{}", data_dir.join("cv.db").display());
    let state = start(Store::open(&url).await?, emit).await?;
    start_change_feed(&state).await;
    Ok(state)
}

/// Builds one tenant's state, restores its providers and starts its mail watcher.
/// Live updates are the caller's: SQLite polls, Postgres uses `forward_changes`.
pub async fn start(store: Store, emit: Emitter) -> Result<Arc<AppState>, AppError> {
    let state = Arc::new(AppState::new(store, EngineSource::Http, emit));
    state.reload_providers().await?;
    start_mail_watcher(state.clone());
    Ok(state)
}

/// Polls trigger-maintained counters for writes from any database connection.
async fn start_change_feed(state: &AppState) {
    let store = state.store.clone();
    let emit = state.emitter();
    // Capture the baseline before returning so immediate caller writes are observed.
    let mut seen = store.change_versions().await.unwrap_or_default();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(250)).await;
            let Ok(now) = store.change_versions().await else {
                continue;
            };
            let tables: Vec<&String> = now
                .iter()
                .filter(|(t, v)| seen.get(*t) != Some(*v))
                .map(|(t, _)| t)
                .collect();
            if !tables.is_empty() {
                emit(DB_CHANGED_EVENT, serde_json::json!({ "tables": tables }));
            }
            seen = now;
        }
    });
}

/// Emits one tenant's Postgres change notifications as `db://changed`, matching the SQLite feed.
pub fn forward_changes(mut changes: ChangeReceiver, emit: Emitter) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(tables) = changes.recv().await {
            emit(DB_CHANGED_EVENT, serde_json::json!({ "tables": tables }));
        }
    })
}

/// Connects immediately when an account exists, otherwise waits for one.
fn start_mail_watcher(state: Arc<AppState>) {
    let Some(rx) = state.watcher.take_receiver() else {
        return;
    };
    let task = tokio::spawn(watch::run_watcher(
        Arc::new(AppWatchDeps {
            state: state.clone(),
        }),
        rx,
        state.watcher.status_handle(),
        state.emitter(),
    ));
    state.watcher.attach(task.abort_handle());
    state.watcher.send(watch::Control::Restart);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bootstrap_opens_store_and_emits_external_changes_without_desktop_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("app-data");
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let state = bootstrap(
            &data_dir,
            Arc::new(move |event, payload| {
                let _ = tx.send((event.to_owned(), payload));
            }),
        )
        .await
        .unwrap();
        assert!(data_dir.join("cv.db").is_file());
        assert!(state.watcher.take_receiver().is_none());
        let deps = AppWatchDeps { state };
        assert!(crate::mail::watch::WatchDeps::connect(&deps)
            .await
            .unwrap()
            .is_none());
        assert!(matches!(
            crate::mail::watch::WatchDeps::cycle(&deps).await,
            Err(crate::mail::cycle::CycleError::NoAccount)
        ));

        let external = Store::open(&format!("sqlite://{}", data_dir.join("cv.db").display()))
            .await
            .unwrap();
        external
            .set_setting("ui.bootstrap-test", "saved")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let (event, payload) = rx.recv().await.unwrap();
                if event == DB_CHANGED_EVENT {
                    assert!(payload["tables"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|t| t == "settings"));
                    break;
                }
            }
        })
        .await
        .expect("change feed did not report the external write");
    }

    #[tokio::test]
    async fn forward_changes_emits_db_changed_payloads() {
        let (changes, rx) = tokio::sync::mpsc::unbounded_channel();
        let (tx, mut events) = tokio::sync::mpsc::unbounded_channel();
        let task = forward_changes(
            rx,
            Arc::new(move |event, payload| {
                let _ = tx.send((event.to_owned(), payload));
            }),
        );
        changes.send(vec!["entry".into(), "bullet".into()]).unwrap();
        drop(changes);
        task.await.unwrap();
        let (event, payload) = events.recv().await.unwrap();
        assert_eq!(event, DB_CHANGED_EVENT);
        assert_eq!(
            payload,
            serde_json::json!({ "tables": ["entry", "bullet"] })
        );
    }

    #[tokio::test]
    async fn bootstrap_returns_directory_errors() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, "not a directory").unwrap();
        assert!(matches!(
            bootstrap(&file, Arc::new(|_, _| {})).await,
            Err(AppError::Io(_))
        ));
    }
}
