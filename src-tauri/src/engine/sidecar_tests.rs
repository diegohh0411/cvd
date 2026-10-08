use super::*;
use crate::engine::{Answer, DecisionEngine, EngineError, Question};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// When a stub server stops accepting connections.
#[derive(Clone, Copy)]
enum Death {
    Never,
    /// Dies right after answering `n` decision requests.
    AfterDecisions(usize),
    /// Dies right after its first 200 on `/health`, before any decision.
    AfterHealthy,
}

#[derive(Clone, Copy)]
struct StubCfg {
    health_503s: usize,
    death: Death,
}

/// A hand-rolled HTTP/1.1 stub of llama-server. Every response is
/// `Connection: close`, so "dying" (dropping the listener) is observable at once.
fn run_stub(port: u16, cfg: StubCfg, alive: Arc<AtomicBool>, hits: Arc<AtomicUsize>) {
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("stub bind");
    thread::spawn(move || {
        let mut health_left_503 = cfg.health_503s;
        let mut decisions = 0usize;
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
            let mut len = 0usize;
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                if h == "\r\n" || h.is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();

            let (status, payload, die) = match path.as_str() {
                "/health" if health_left_503 > 0 => {
                    health_left_503 -= 1;
                    ("503 Service Unavailable", r#"{"error":{"code":503,"message":"Loading model"}}"#.to_string(), false)
                }
                "/health" => ("200 OK", r#"{"status":"ok"}"#.to_string(), matches!(cfg.death, Death::AfterHealthy)),
                "/v1/systemone" => {
                    hits.fetch_add(1, Ordering::SeqCst);
                    decisions += 1;
                    let req: serde_json::Value = serde_json::from_slice(&body).unwrap();
                    let answers: serde_json::Map<String, serde_json::Value> = req["questions"]
                        .as_object()
                        .unwrap()
                        .keys()
                        .map(|k| (k.clone(), serde_json::json!({"type": "noul", "noul": 0.7})))
                        .collect();
                    let die = matches!(cfg.death, Death::AfterDecisions(n) if decisions >= n);
                    ("200 OK", serde_json::json!({ "answers": answers }).to_string(), die)
                }
                _ => ("404 Not Found", "{}".to_string(), false),
            };
            if die {
                alive.store(false, Ordering::SeqCst);
            }
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                payload.len()
            );
            let _ = stream.flush();
            if die {
                return; // drops the listener: the port now refuses connections
            }
        }
    });
}

struct StubChild {
    alive: Arc<AtomicBool>,
}

impl Child for StubChild {
    fn kill(&mut self) {
        self.alive.store(false, Ordering::SeqCst);
    }
    fn alive(&mut self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }
}

/// Starts one stub per spawn, using `cfgs[i]` for the i-th spawn (last one repeats).
struct StubLauncher {
    cfgs: Vec<StubCfg>,
    spawns: Arc<AtomicUsize>,
    hits: Arc<AtomicUsize>,
    args: Arc<Mutex<Vec<Vec<String>>>>,
}

impl StubLauncher {
    fn new(cfgs: Vec<StubCfg>) -> Self {
        Self {
            cfgs,
            spawns: Arc::default(),
            hits: Arc::default(),
            args: Arc::default(),
        }
    }
}

impl Launcher for StubLauncher {
    fn spawn(&self, model: &Path, port: u16) -> Result<Box<dyn Child>, EngineError> {
        let i = self.spawns.fetch_add(1, Ordering::SeqCst);
        self.args.lock().unwrap().push(server_args(model, port));
        let cfg = self.cfgs[i.min(self.cfgs.len() - 1)];
        let alive = Arc::new(AtomicBool::new(true));
        run_stub(port, cfg, alive.clone(), self.hits.clone());
        Ok(Box::new(StubChild { alive }))
    }
}

const HEALTHY: StubCfg = StubCfg { health_503s: 0, death: Death::Never };

fn questions() -> BTreeMap<String, Question> {
    let mut qs = BTreeMap::new();
    qs.insert("q".to_string(), Question::Noul { instructions: "Rust?".into() });
    qs
}

fn sidecar(l: StubLauncher) -> (LlamaSidecar, Arc<AtomicUsize>, Arc<Mutex<Vec<Vec<String>>>>) {
    let (spawns, args) = (l.spawns.clone(), l.args.clone());
    (LlamaSidecar::new(Box::new(l), PathBuf::from("/models/clef.gguf")), spawns, args)
}

#[tokio::test]
async fn starts_lazily() {
    let (s, spawns, _) = sidecar(StubLauncher::new(vec![HEALTHY]));
    assert_eq!(spawns.load(Ordering::SeqCst), 0);
    let got = s.decide("state", &questions()).await.unwrap();
    assert_eq!(got["q"], Answer::Noul { p: 0.7 });
    assert_eq!(spawns.load(Ordering::SeqCst), 1);
    s.decide("state", &questions()).await.unwrap();
    assert_eq!(spawns.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn waits_for_health() {
    let l = StubLauncher::new(vec![StubCfg { health_503s: 2, death: Death::Never }]);
    let (s, spawns, _) = sidecar(l);
    assert_eq!(s.decide("state", &questions()).await.unwrap()["q"], Answer::Noul { p: 0.7 });
    assert_eq!(spawns.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn restarts_once_then_succeeds() {
    let dies_after_one = StubCfg { health_503s: 0, death: Death::AfterDecisions(1) };
    let (s, spawns, _) = sidecar(StubLauncher::new(vec![dies_after_one, HEALTHY]));
    s.decide("state", &questions()).await.unwrap();
    assert_eq!(s.decide("state", &questions()).await.unwrap()["q"], Answer::Noul { p: 0.7 });
    assert_eq!(spawns.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn restarts_once_then_fails() {
    let dies_after_one = StubCfg { health_503s: 0, death: Death::AfterDecisions(1) };
    let dies_at_once = StubCfg { health_503s: 0, death: Death::AfterHealthy };
    let (s, spawns, _) = sidecar(StubLauncher::new(vec![dies_after_one, dies_at_once]));
    s.decide("state", &questions()).await.unwrap();
    let err = s.decide("state", &questions()).await.unwrap_err();
    assert!(matches!(err, EngineError::Unavailable(_)), "{err:?}");
    assert_eq!(spawns.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn binds_loopback() {
    let (s, _, args) = sidecar(StubLauncher::new(vec![HEALTHY]));
    s.decide("state", &questions()).await.unwrap();
    let args = args.lock().unwrap();
    let a = &args[0];
    let host = a.iter().position(|x| x == "--host").expect("--host");
    assert_eq!(a[host + 1], "127.0.0.1");
    let ub = a.iter().position(|x| x == "-ub").expect("-ub");
    assert_eq!(a[ub + 1], "16384");
    assert_eq!(a[a.iter().position(|x| x == "-m").unwrap() + 1], "/models/clef.gguf");
}

#[tokio::test]
async fn overflow_body_maps_to_context_overflow() {
    // A 500 with a JSON error body is parsed, not treated as a dead server.
    let port = free_port().unwrap();
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let mut buf = [0u8; 65536];
            let n = stream.read(&mut buf).unwrap();
            let req = String::from_utf8_lossy(&buf[..n]);
            let (status, body) = if req.starts_with("GET /health") {
                ("200 OK", r#"{"status":"ok"}"#)
            } else {
                ("500 Internal Server Error", r#"{"error":{"code":500,"message":"input (1921 tokens) is too large to process. increase the physical batch size (current batch size: 512)","type":"server_error"}}"#)
            };
            let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    struct Fixed;
    impl Launcher for Fixed {
        fn spawn(&self, _: &Path, _: u16) -> Result<Box<dyn Child>, EngineError> {
            Ok(Box::new(StubChild { alive: Arc::new(AtomicBool::new(true)) }))
        }
    }
    let s = LlamaSidecar::new(Box::new(Fixed), PathBuf::from("m")).with_fixed_port(port);
    assert_eq!(s.decide("state", &questions()).await, Err(EngineError::ContextOverflow));
}

#[tokio::test]
async fn shutdown_kills_child_and_refuses_new_work() {
    let l = StubLauncher::new(vec![HEALTHY]);
    let (s, spawns, _) = sidecar(l);
    s.decide("state", &questions()).await.unwrap();
    s.shutdown();
    assert!(matches!(s.decide("state", &questions()).await, Err(EngineError::Unavailable(_))));
    assert_eq!(spawns.load(Ordering::SeqCst), 1);
}

#[cfg(feature = "it-sidecar")]
#[tokio::test]
async fn real_server_shape() {
    struct ProcessLauncher(PathBuf);
    struct ProcessChild(std::process::Child);
    impl Child for ProcessChild {
        fn kill(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
        fn alive(&mut self) -> bool {
            matches!(self.0.try_wait(), Ok(None))
        }
    }
    impl Launcher for ProcessLauncher {
        fn spawn(&self, model: &Path, port: u16) -> Result<Box<dyn Child>, EngineError> {
            let c = std::process::Command::new(&self.0)
                .args(server_args(model, port))
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .map_err(|e| EngineError::Unavailable(e.to_string()))?;
            Ok(Box::new(ProcessChild(c)))
        }
    }
    let model = std::env::var("CVD_TEST_MODEL").expect("set CVD_TEST_MODEL to a Clef .gguf");
    let bin = std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/binaries"))
        .expect("run scripts/fetch-llama-server.sh first")
        .flatten()
        .map(|e| e.path())
        .find(|p| p.file_name().unwrap().to_string_lossy().starts_with("llama-server-"))
        .expect("llama-server-<triple> in binaries/");
    let s = LlamaSidecar::new(Box::new(ProcessLauncher(bin)), PathBuf::from(model));
    let got = s
        .decide("Senior Rust engineer, async services, Tokio, PostgreSQL.", &questions())
        .await
        .unwrap();
    s.shutdown();
    assert_eq!(got.len(), 1);
    match got["q"] {
        Answer::Noul { p } => assert!((0.0..=1.0).contains(&p), "{p}"),
        ref other => panic!("{other:?}"),
    }
    eprintln!("real_server_shape: {got:?}");
}
