//! Throwaway spike for docs/superpowers/specs/2026-10-17-auto-apply-design.md.
//! Not part of the product; delete once the real autofill module lands.

use base64::Engine as _;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "apply-test";
const INIT: &str = include_str!("spike_init.js");
const LOG: &str = "/tmp/cvd-spike.log";

fn log(line: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(LOG) {
        let _ = writeln!(f, "{line}");
    }
    eprintln!("[spike] {line}");
}

pub fn open_apply(app: &AppHandle, url: &str) -> Result<(), Box<dyn std::error::Error>> {
    let parsed = url.parse()?;
    let win = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::External(parsed))
        .title("apply-test")
        .inner_size(1100.0, 900.0)
        .initialization_script(INIT)
        .build()?;
    // Dev loop: eval whatever is dropped into /tmp/cvd-spike-eval.js.
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if let Ok(js) = std::fs::read_to_string("/tmp/cvd-spike-eval.js") {
            let _ = std::fs::remove_file("/tmp/cvd-spike-eval.js");
            let _ = win.eval(js);
        }
    });
    Ok(())
}

#[tauri::command]
pub fn spike_open_apply(app: AppHandle, url: String) -> Result<(), String> {
    if !cfg!(debug_assertions) {
        return Err("dev only".into());
    }
    open_apply(&app, &url).map_err(|e| e.to_string())
}

fn tiny_pdf() -> Vec<u8> {
    b"%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj\n3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]>>endobj\ntrailer<</Root 1 0 R/Size 4>>\n%%EOF\n".to_vec()
}

/// The single command a remote page may call.
#[tauri::command]
pub fn autofill_bridge(window: tauri::WebviewWindow, msg: Value) -> Result<Value, String> {
    // Treat as hostile data: cap size, never use it for paths.
    let raw = msg.to_string();
    if raw.len() > 400_000 {
        return Err("too large".into());
    }
    let kind = msg.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "fields" => {
            log(&format!("origin={} fields={}", window.url().map(|u| u.to_string()).unwrap_or_default(), raw));
            // (a) fill the first text field through eval
            let fill = json!({"key": "first_name", "value": "Diego"});
            let _ = window.eval(format!("window.__spikeFill({fill})"));
            // (b) resume upload, bytes as base64
            let b64 = base64::engine::general_purpose::STANDARD.encode(tiny_pdf());
            let _ = window.eval(format!(
                "window.__spikeFile({})",
                json!({"name": "Diego_Hernandez_CV.pdf", "b64": b64})
            ));
            Ok(json!({"ok": true}))
        }
        _ => {
            log(&format!("{kind}: {raw}"));
            Ok(json!({"ok": true}))
        }
    }
}

#[allow(dead_code)]
fn _unused(app: &AppHandle) {
    let _ = app.get_webview_window(LABEL);
}
