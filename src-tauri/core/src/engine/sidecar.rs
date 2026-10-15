//! Production `DecisionEngine`: a lazily started `llama-server` sidecar bound to
//! 127.0.0.1 on a random free port, restarted at most once per request.
use super::types::{Answer, EngineError, Question};
use super::wire::{parse_wire_response, wire_request};
use super::DecisionEngine;
use crate::models::ModelKind;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const HEALTH_POLL: Duration = Duration::from_millis(250);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(60);
/// 128 `noul` questions took ~22 s in the spike; leave generous headroom.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
/// Clef evaluates the whole prompt in one physical batch; the default 512 overflows.
const BATCH: &str = "16384";

pub trait Launcher: Send + Sync {
    fn spawn(&self, model: &Path, port: u16, kind: ModelKind) -> Result<Box<dyn Child>, EngineError>;
}

pub trait Child: Send {
    fn kill(&mut self);
    fn alive(&mut self) -> bool;
}

/// Context for chat models: room for a research agent's tool results.
const CHAT_CTX: &str = "32768";

/// Command-line arguments for `llama-server`. Chat models also need Jinja
/// chat templates (so tool calls are parsed) and a larger context.
pub fn server_args(model: &Path, port: u16, kind: ModelKind) -> Vec<String> {
    let mut args = vec![
        "-m".into(),
        model.to_string_lossy().into_owned(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "-b".into(),
        BATCH.into(),
        "-ub".into(),
        BATCH.into(),
    ];
    if kind == ModelKind::Chat {
        args.extend(["--jinja".into(), "-c".into(), CHAT_CTX.into()]);
    }
    args
}

/// Asks the OS for a free loopback port, then releases it for the server.
pub fn free_port() -> Result<u16, EngineError> {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| EngineError::Unavailable(format!("no free port: {e}")))
}

/// Outcome of one start-and-request attempt that did not succeed.
enum Failure {
    /// The server is dead or unreachable; a restart may help.
    Dead(String),
    /// Restarting will not help (bad response, overflow, timeout, spawn error).
    Final(EngineError),
}

pub struct LlamaSidecar {
    launcher: Box<dyn Launcher>,
    model_path: PathBuf,
    kind: ModelKind,
    http: reqwest::Client,
    /// Port of the running server. Holding this lock serialises requests and restarts.
    port: tokio::sync::Mutex<Option<u16>>,
    /// Kept apart from `port` so `shutdown` never waits behind an in-flight request.
    child: std::sync::Mutex<Option<Box<dyn Child>>>,
    shut_down: AtomicBool,
    #[cfg(test)]
    fixed_port: Option<u16>,
}

impl LlamaSidecar {
    /// Spawns nothing; the server starts on the first `decide`.
    pub fn new(launcher: Box<dyn Launcher>, model_path: PathBuf) -> Self {
        Self {
            launcher,
            model_path,
            kind: ModelKind::Decision,
            http: reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .no_proxy()
                .build()
                .expect("reqwest client"),
            port: tokio::sync::Mutex::new(None),
            child: std::sync::Mutex::new(None),
            shut_down: AtomicBool::new(false),
            #[cfg(test)]
            fixed_port: None,
        }
    }

    /// Serves a chat model (OpenAI-compatible `/v1/chat/completions`) instead of decisions.
    pub fn with_kind(mut self, kind: ModelKind) -> Self {
        self.kind = kind;
        self
    }

    /// Starts the server if needed (restarting a dead one once) and returns its port.
    pub async fn ensure_started(&self) -> Result<u16, EngineError> {
        let mut slot = self.port.lock().await;
        let mut restarted = false;
        loop {
            if self.shut_down.load(Ordering::SeqCst) {
                return Err(EngineError::Unavailable("llama-server was shut down".into()));
            }
            match *slot {
                Some(p) if self.child_alive() => return Ok(p),
                Some(_) => {
                    self.kill_child();
                    *slot = None;
                }
                None => {}
            }
            match self.start().await {
                Ok(p) => *slot = Some(p),
                Err(Failure::Final(e)) => return Err(e),
                Err(Failure::Dead(why)) => {
                    self.kill_child();
                    if restarted {
                        return Err(EngineError::Unavailable(why));
                    }
                    restarted = true;
                }
            }
        }
    }

    #[cfg(test)]
    fn with_fixed_port(mut self, port: u16) -> Self {
        self.fixed_port = Some(port);
        self
    }

    /// Kills the server. Later `decide` calls fail with `Unavailable`.
    pub fn shutdown(&self) {
        self.shut_down.store(true, Ordering::SeqCst);
        self.kill_child();
    }

    fn kill_child(&self) {
        if let Some(mut c) = self.child.lock().unwrap_or_else(|e| e.into_inner()).take() {
            c.kill();
        }
    }

    /// True while a started server process is alive.
    pub fn is_running(&self) -> bool {
        self.child_alive()
    }

    fn child_alive(&self) -> bool {
        self.child
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
            .is_some_and(|c| c.alive())
    }

    fn pick_port(&self) -> Result<u16, EngineError> {
        #[cfg(test)]
        if let Some(p) = self.fixed_port {
            return Ok(p);
        }
        free_port()
    }

    async fn start(&self) -> Result<u16, Failure> {
        let port = self.pick_port().map_err(Failure::Final)?;
        let child = self.launcher.spawn(&self.model_path, port, self.kind).map_err(Failure::Final)?;
        {
            let mut slot = self.child.lock().unwrap_or_else(|e| e.into_inner());
            *slot = Some(child);
            if self.shut_down.load(Ordering::SeqCst) {
                if let Some(mut c) = slot.take() {
                    c.kill();
                }
                return Err(Failure::Final(EngineError::Unavailable("decision engine was shut down".into())));
            }
        }
        let url = format!("http://127.0.0.1:{port}/health");
        let deadline = Instant::now() + HEALTH_TIMEOUT;
        loop {
            if !self.child_alive() {
                return Err(Failure::Dead("llama-server exited during startup".into()));
            }
            if let Ok(r) = self.http.get(&url).timeout(Duration::from_secs(2)).send().await {
                if r.status().is_success() {
                    return Ok(port);
                }
            }
            if Instant::now() >= deadline {
                return Err(Failure::Dead("llama-server did not become healthy within 60 s".into()));
            }
            tokio::time::sleep(HEALTH_POLL).await;
        }
    }

    async fn attempt(&self, slot: &mut Option<u16>, body: &Value) -> Result<BTreeMap<String, Answer>, Failure> {
        let port = match *slot {
            Some(_) if !self.child_alive() => return Err(Failure::Dead("llama-server exited".into())),
            Some(p) => p,
            None => {
                let p = self.start().await?;
                *slot = Some(p);
                p
            }
        };
        let resp = self
            .http
            .post(format!("http://127.0.0.1:{port}/v1/systemone"))
            .json(body)
            .send()
            .await
            .map_err(transport)?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(transport)?;
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(v) => parse_wire_response(v).map_err(Failure::Final),
            Err(e) if status.is_success() => Err(Failure::Final(EngineError::BadResponse(format!("invalid JSON: {e}")))),
            Err(_) => Err(Failure::Final(EngineError::BadResponse(format!(
                "HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            )))),
        }
    }
}

fn transport(e: reqwest::Error) -> Failure {
    if e.is_timeout() {
        Failure::Final(EngineError::Unavailable(format!("request timed out: {e}")))
    } else {
        Failure::Dead(format!("llama-server unreachable: {e}"))
    }
}

#[async_trait]
impl DecisionEngine for LlamaSidecar {
    async fn decide(
        &self,
        state: &str,
        questions: &BTreeMap<String, Question>,
    ) -> Result<BTreeMap<String, Answer>, EngineError> {
        let body = wire_request(state, questions);
        let mut slot = self.port.lock().await;
        let mut restarted = false;
        loop {
            if self.shut_down.load(Ordering::SeqCst) {
                return Err(EngineError::Unavailable("decision engine was shut down".into()));
            }
            match self.attempt(&mut slot, &body).await {
                Ok(answers) => return Ok(answers),
                Err(Failure::Final(e)) => return Err(e),
                Err(Failure::Dead(why)) => {
                    self.kill_child();
                    *slot = None;
                    if restarted {
                        return Err(EngineError::Unavailable(why));
                    }
                    restarted = true;
                }
            }
        }
    }
}

impl Drop for LlamaSidecar {
    fn drop(&mut self) {
        self.kill_child();
    }
}

#[cfg(test)]
#[path = "sidecar_tests.rs"]
mod tests;
