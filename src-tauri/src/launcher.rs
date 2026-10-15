//! The Tauri side of the decision engine: spawns `llama-server` as a bundled sidecar.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use cvd_core::engine::{server_args, Child, EngineError, Launcher};
use cvd_core::models::ModelKind;

/// Launches the bundled `binaries/llama-server-<triple>` through the Tauri shell plugin.
pub struct TauriLauncher(pub tauri::AppHandle);

struct TauriChild {
    child: Option<tauri_plugin_shell::process::CommandChild>,
    alive: Arc<AtomicBool>,
}

impl Child for TauriChild {
    fn kill(&mut self) {
        if let Some(c) = self.child.take() {
            let _ = c.kill();
        }
        self.alive.store(false, Ordering::SeqCst);
    }
    fn alive(&mut self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }
}

impl Launcher for TauriLauncher {
    fn spawn(&self, model: &Path, port: u16, kind: ModelKind) -> Result<Box<dyn Child>, EngineError> {
        use tauri_plugin_shell::{process::CommandEvent, ShellExt};
        let unavailable = |e: tauri_plugin_shell::Error| EngineError::Unavailable(format!("cannot start llama-server: {e}"));
        let (mut rx, child) = self
            .0
            .shell()
            .sidecar("llama-server")
            .map_err(unavailable)?
            .args(server_args(model, port, kind))
            .spawn()
            .map_err(unavailable)?;
        let alive = Arc::new(AtomicBool::new(true));
        let flag = alive.clone();
        // Drain the output channel (it is bounded; an undrained one stalls the
        // server's logging) and notice when the process exits.
        tauri::async_runtime::spawn(async move {
            while let Some(ev) = rx.recv().await {
                if matches!(ev, CommandEvent::Terminated(_)) {
                    break;
                }
            }
            flag.store(false, Ordering::SeqCst);
        });
        Ok(Box::new(TauriChild { child: Some(child), alive }))
    }
}

