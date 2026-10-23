//! `POST /api/rpc/:command`: the same names and camelCase args as the Tauri commands.
use serde::de::DeserializeOwned;
use serde_json::Value;

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

/// Desktop-only commands; the web build hides their UI.
pub use cvd_core::commands::DESKTOP_ONLY;

/// Why a call failed; `Unknown` maps to 404, everything else keeps the `AppError` body.
#[derive(Debug, PartialEq)]
pub enum RpcError {
    Unknown(String),
    App(AppError),
}
impl From<AppError> for RpcError {
    fn from(e: AppError) -> Self {
        Self::App(e)
    }
}

fn args<T: DeserializeOwned>(cmd: &str, v: Value) -> Result<T, RpcError> {
    // Tauri accepts a missing args object for commands without required args.
    let v = if v.is_null() {
        Value::Object(Default::default())
    } else {
        v
    };
    serde_json::from_value(v)
        .map_err(|e| AppError::Io(format!("invalid arguments for {cmd}: {e}")).into())
}

fn json<T: serde::Serialize>(v: T) -> Result<Value, RpcError> {
    serde_json::to_value(v).map_err(|e| AppError::Io(e.to_string()).into())
}

/// Expands the shared registry (`cvd_core::for_each_command!`) into [`dispatch_shared`].
macro_rules! server_commands {
    (
        desktop_only: [$($desktop:ident ($($darg:ident),*)),* $(,)?];
        per_transport: [$($per:ident ($($parg:ident),*)),* $(,)?];
        $st:ident;
        $($name:ident ($($arg:ident : $ty:ty),* $(,)?) -> $ret:ty => $body:expr;)*
    ) => {
        async fn dispatch_shared($st: &AppState, cmd: &str, raw: Value) -> Result<Value, RpcError> {
            match cmd {
                $(stringify!($name) => {
                    #[derive(serde::Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    #[allow(dead_code)]
                    struct Args { $($arg: $ty),* }
                    #[allow(unused_variables)]
                    let Args { $($arg),* } = args::<Args>(cmd, raw)?;
                    let out: Result<$ret, AppError> = $body;
                    json(out?)
                })*
                _ => Err(RpcError::Unknown(cmd.to_owned())),
            }
        }
    };
}
cvd_core::for_each_command!(server_commands);

/// Every command the server dispatches.
pub fn commands() -> impl Iterator<Item = &'static str> {
    cvd_core::commands::SHARED
        .iter()
        .map(|(name, _)| *name)
        .chain(cvd_core::commands::PER_TRANSPORT.iter().copied())
}

pub async fn dispatch(st: &AppState, cmd: &str, raw: Value) -> Result<Value, RpcError> {
    match cmd {
        // The browser downloads the bytes; there is no server-side file to record.
        "export_cv" => {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Args {
                run_id: i64,
                selection: Selection,
            }
            let Args { run_id, selection } = args::<Args>(cmd, raw)?;
            json(services::export_cv(st, run_id, selection).await?)
        }
        _ if DESKTOP_ONLY.contains(&cmd) => Err(AppError::Io(format!(
            "{cmd} is not available on the web; use the desktop app."
        ))
        .into()),
        _ => dispatch_shared(st, cmd, raw).await,
    }
}
