mod commands;
mod company;
#[allow(dead_code)]
mod mail;
// Some items in these modules are only used by tests or by later plans.
#[allow(dead_code)]
mod engine;
#[allow(dead_code)]
mod matcher;
#[allow(dead_code)]
mod models;
mod providers;
#[allow(dead_code)]
mod render;
mod search;
mod state;
#[allow(dead_code)]
pub mod store;
pub mod mcp;

use std::sync::Arc;

use tauri::{Emitter as _, Manager};

use commands::ipc;
use state::{AppState, EngineSource};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let url = format!("sqlite://{}", dir.join("cv.db").display());
            let store = tauri::async_runtime::block_on(store::Store::open(&url))?;
            let handle = app.handle().clone();
            let emit_handle = handle.clone();
            let state = AppState::new(
                store,
                dir.join("models"),
                EngineSource::Sidecar(handle),
                Arc::new(move |event: &str, payload: serde_json::Value| {
                    let _ = emit_handle.emit(event, payload);
                }),
            );
            tauri::async_runtime::block_on(state.reload_providers())?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ipc::get_profile,
            ipc::save_profile,
            ipc::list_entries,
            ipc::save_entry,
            ipc::delete_entry,
            ipc::list_skills,
            ipc::create_job,
            ipc::update_job_tracking,
            ipc::list_job_statuses,
            ipc::create_job_status,
            ipc::update_job_status,
            ipc::delete_job_status,
            ipc::reorder_job_statuses,
            ipc::list_jobs,
            ipc::get_job,
            ipc::start_match,
            ipc::cancel_match,
            ipc::resume_match,
            ipc::list_runs,
            ipc::get_scores,
            ipc::list_cvs,
            ipc::get_ui_setting,
            ipc::set_ui_setting,
            ipc::engine_status,
            ipc::cancel_download,
            ipc::get_ranked,
            ipc::fit_cv,
            ipc::export_cv,
            ipc::system_ram_gb,
            ipc::list_models,
            ipc::download_model,
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
            ipc::list_mail_folders,
            ipc::delete_mail_account,
            ipc::test_mail_account,
            ipc::start_scan,
            ipc::get_scan_status,
            ipc::list_scan_results,
            ipc::list_pending_reviews,
            ipc::apply_suggestion,
            ipc::dismiss_suggestion,
            ipc::set_provider_key,
            ipc::delete_provider_key,
            ipc::provider_key_status,
            ipc::list_remote_models,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                // Kills the llama-server sidecar; a no-op if it was never built.
                if let Some(state) = app.try_state::<AppState>() {
                    state.shutdown();
                }
            }
        });
}
