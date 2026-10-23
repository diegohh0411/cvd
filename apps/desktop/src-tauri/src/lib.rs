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
        .invoke_handler(ipc::handler!())
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_, _| {});
}
