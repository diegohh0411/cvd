#[allow(dead_code)]
mod store;
#[allow(dead_code)]
mod models;
#[allow(dead_code)]
mod engine;
#[allow(dead_code)]
mod matcher;
#[allow(dead_code)]
mod render;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![greet])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                shutdown_sidecar(app);
            }
        });
}

/// Kills the llama-server sidecar on exit. Task 9 manages an
/// `Arc<engine::LlamaSidecar>` (inside AppState); until then nothing is managed.
fn shutdown_sidecar(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(sidecar) = app.try_state::<std::sync::Arc<engine::LlamaSidecar>>() {
        sidecar.shutdown();
    }
}
