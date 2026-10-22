use tauri::{Emitter as _, Manager};

mod autofill;
mod ipc;
use cvd_core::{core, engine, services as commands, state};
use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let emit_handle = app.handle().clone();
            let state = tauri::async_runtime::block_on(core::bootstrap(
                &dir,
                Arc::new(move |event: &str, payload: serde_json::Value| {
                    let _ = emit_handle.emit(event, payload);
                }),
            ))?;
            app.manage(state);
            app.manage(autofill::window::Sessions::default());
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
            ipc::save_file_bytes,
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
