mod autofill;
mod commands;
mod company;
mod job_search;
#[allow(dead_code)]
mod mail;
// Some items in these modules are only used by tests or by later plans.
#[allow(dead_code)]
mod engine;
#[allow(dead_code)]
mod matcher;
pub mod mcp;
mod providers;
#[allow(dead_code)]
mod render;
mod search;
mod state;
#[allow(dead_code)]
pub mod store;
mod web;

use std::sync::Arc;

use tauri::{Emitter as _, Manager};

use commands::ipc;
use state::{AppState, EngineSource};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let url = format!("sqlite://{}", dir.join("cv.db").display());
            let store = tauri::async_runtime::block_on(store::Store::open(&url))?;
            let handle = app.handle().clone();
            let emit_handle = handle.clone();
            let state = AppState::new(
                store,
                EngineSource::Http,
                Arc::new(move |event: &str, payload: serde_json::Value| {
                    let _ = emit_handle.emit(event, payload);
                }),
            );
            tauri::async_runtime::block_on(state.reload_providers())?;
            app.manage(state);
            start_mail_watcher(app.handle());
            start_change_feed(app.handle());
            app.manage(autofill::window::Sessions::default());
            app.manage(autofill::agent::live::Agents::default());
            // Dev hook: open an apply window on any supported posting, with no job attached.
            #[cfg(debug_assertions)]
            if let Ok(url) = std::env::var("CVD_APPLY_URL") {
                autofill::window::open_window(app.handle(), 0, &url, "Apply (dev)")?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::get_local_connection,
            ipc::save_local_connection,
            ipc::test_local_connection,
            ipc::get_profile,
            ipc::save_profile,
            ipc::list_entries,
            ipc::save_entry,
            ipc::delete_entry,
            ipc::list_skills,
            ipc::create_job,
            ipc::update_job_tracking,
            ipc::update_job_description,
            ipc::fetch_job_description,
            ipc::list_job_statuses,
            ipc::create_job_status,
            ipc::update_job_status,
            ipc::delete_job_status,
            ipc::reorder_job_statuses,
            ipc::list_job_views,
            ipc::create_job_view,
            ipc::update_job_view,
            ipc::delete_job_view,
            ipc::list_jobs,
            ipc::search_jobs,
            ipc::get_job,
            ipc::start_match,
            ipc::cancel_match,
            ipc::list_runs,
            ipc::get_scores,
            ipc::list_cvs,
            ipc::get_ui_setting,
            ipc::set_ui_setting,
            ipc::get_apply_defaults,
            ipc::save_apply_defaults,
            ipc::apply_facts,
            ipc::engine_status,
            ipc::get_ranked,
            ipc::fit_cv,
            ipc::export_cv,
            ipc::set_active_model,
            ipc::set_research_model,
            ipc::skip_research,
            ipc::rescore,
            ipc::get_company,
            ipc::get_job_company,
            ipc::edit_company,
            ipc::refresh_company,
            ipc::get_research_model,
            ipc::get_mail_account,
            ipc::save_mail_account,
            ipc::delete_mail_account,
            ipc::test_mail_account,
            ipc::start_scan,
            ipc::get_scan_status,
            ipc::list_pending_reviews,
            ipc::list_job_emails,
            ipc::apply_suggestion,
            ipc::dismiss_suggestion,
            ipc::get_watcher_status,
            ipc::retry_watcher,
            ipc::list_inbox,
            ipc::unread_count,
            ipc::mark_read,
            ipc::mark_all_read,
            ipc::undo_applied,
            ipc::set_provider_key,
            ipc::delete_provider_key,
            ipc::provider_key_status,
            ipc::list_connections,
            ipc::add_connection,
            ipc::rename_connection,
            ipc::list_remote_models,
            ipc::start_apply,
            autofill::window::autofill_bridge,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, _| {});
}

/// Runs the Gmail watcher for the life of the app. It connects right away
/// when an account is saved, and otherwise waits for one.
/// Polls the trigger-maintained change counters and tells the UI which tables changed.
fn start_change_feed(handle: &tauri::AppHandle) {
    let st = handle.state::<AppState>();
    let store = st.store.clone();
    let emit = st.emitter();
    tauri::async_runtime::spawn(async move {
        let mut seen = store.change_versions().await.unwrap_or_default();
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let Ok(now) = store.change_versions().await else {
                continue;
            };
            let tables: Vec<&String> = now
                .iter()
                .filter(|(t, v)| seen.get(*t) != Some(*v))
                .map(|(t, _)| t)
                .collect();
            if !tables.is_empty() {
                emit(state::DB_CHANGED_EVENT, serde_json::json!({ "tables": tables }));
            }
            seen = now;
        }
    });
}

fn start_mail_watcher(handle: &tauri::AppHandle) {
    let st = handle.state::<AppState>();
    let Some(rx) = st.watcher.take_receiver() else {
        return;
    };
    let deps = Arc::new(mail::cycle::AppWatchDeps {
        handle: handle.clone(),
    });
    tauri::async_runtime::spawn(mail::watch::run_watcher(
        deps,
        rx,
        st.watcher.status_handle(),
        st.emitter(),
    ));
    st.watcher.send(mail::watch::Control::Restart);
}
