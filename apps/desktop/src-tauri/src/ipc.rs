//! `#[tauri::command]` wrappers. Shared commands expand from the registry in
//! `cvd_core::for_each_command!`; only desktop-only and per-transport ones are written here.
use tauri::State;

use cvd_core::services::{self, *};
use cvd_core::{
    autofill::{ApplyDefaults, Fact},
    mail::watch::WatcherStatus,
    matcher::Ranked,
    providers::ProviderModels,
    render::fit::{Override, Selection},
    state::{AppState, EngineStatus},
    store::{types::*, InboxFilter},
};
type Res<T> = Result<T, AppError>;

type S<'a> = State<'a, std::sync::Arc<AppState>>;

/// One `#[tauri::command]` per shared command, plus `handler!()`: the
/// `generate_handler!` list of every command in the registry.
macro_rules! desktop_commands {
    (
        desktop_only: [$($desktop:ident ($($darg:ident),*)),* $(,)?];
        per_transport: [$($per:ident ($($parg:ident),*)),* $(,)?];
        $st:ident;
        $($name:ident ($($arg:ident : $ty:ty),* $(,)?) -> $ret:ty => $body:expr;)*
    ) => {
        $(
            #[tauri::command]
            #[allow(unused_variables)]
            pub async fn $name($st: S<'_>, $($arg: $ty),*) -> Res<$ret> {
                let $st: &AppState = &$st;
                $body
            }
        )*

        macro_rules! handler {
            () => {
                tauri::generate_handler![
                    $(crate::ipc::$name,)*
                    $(crate::ipc::$desktop,)*
                    $(crate::ipc::$per,)*
                ]
            };
        }
        pub(crate) use handler;
    };
}
cvd_core::for_each_command!(desktop_commands);

#[tauri::command]
pub async fn get_local_connection(st: S<'_>) -> Res<LocalConnectionView> {
    cvd_core::services::get_local_connection(&st).await
}
#[tauri::command]
pub async fn save_local_connection(st: S<'_>, base_url: String, key: Option<String>) -> Res<()> {
    cvd_core::services::save_local_connection(&st, &base_url, key).await
}
#[tauri::command]
pub async fn test_local_connection(st: S<'_>, model: String, role: String) -> Res<()> {
    cvd_core::services::test_local_connection(&st, &model, &role).await
}
#[tauri::command]
pub async fn start_apply(app: tauri::AppHandle, st: S<'_>, job_id: i64) -> Res<bool> {
    crate::autofill::window::start_apply(&app, &st, job_id).await
}
#[tauri::command]
pub async fn export_cv(st: S<'_>, run_id: i64, selection: Selection, path: String) -> Res<i64> {
    use cvd_core::services::{delete_cv, export_cv, set_cv_file_ref, validate_export_path};
    validate_export_path(&path)?;
    let cv = export_cv(&st, run_id, selection).await?;
    if let Err(e) = tokio::fs::write(&path, &cv.pdf).await {
        delete_cv(&st, cv.id).await?;
        return Err(e.into());
    }
    set_cv_file_ref(&st, cv.id, &path).await?;
    Ok(cv.id)
}
/// The native save dialog owns the destination; callers cannot write an arbitrary path.
#[tauri::command]
pub async fn save_file_bytes(
    app: tauri::AppHandle,
    bytes: Vec<u8>,
    suggested_name: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .set_file_name(&suggested_name)
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The single command remote pages may call (see `capabilities/apply.json`).
#[tauri::command]
pub async fn autofill_bridge(
    window: tauri::WebviewWindow,
    st: S<'_>,
    sessions: State<'_, crate::autofill::window::Sessions>,
    msg: serde_json::Value,
) -> Res<()> {
    crate::autofill::window::handle_bridge(&window, &st, &sessions, msg).await
}

#[cfg(test)]
mod tests {
    /// Parameters Tauri injects rather than reading from the payload.
    const INJECTED: &[&str] = &["st", "app", "window", "sessions"];

    /// Hand-written commands take exactly the arguments the registry lists.
    #[test]
    fn handwritten_commands_match_the_registry() {
        let src = include_str!("ipc.rs");
        for (name, args) in cvd_core::commands::HANDWRITTEN {
            let sig = src
                .split_once(&format!("pub async fn {name}("))
                .unwrap_or_else(|| panic!("{name} is not defined in ipc.rs"))
                .1;
            let params: Vec<&str> = sig[..sig.find(')').unwrap()]
                .split(',')
                // `name: Type`; skips generic-argument fragments like `crate::x`.
                .filter_map(|p| p.split_once(':').filter(|(_, ty)| !ty.starts_with(':')))
                .map(|(p, _)| p.trim())
                .filter(|p| !p.is_empty() && !INJECTED.contains(p))
                .collect();
            assert_eq!(&params, args, "{name}");
        }
    }
}
