pub mod registry;

#[cfg(test)]
mod tests;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use hf_hub::api::tokio::{ApiBuilder, Progress};
use hf_hub::Cache;
use sha2::{Digest, Sha256};
use sysinfo::System;

pub use registry::{ModelKind, ModelSpec, REGISTRY};

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("network error: {0}")]
    Network(String),
    #[error("checksum mismatch")]
    ChecksumMismatch,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Total physical RAM in whole GB (rounded to nearest).
pub fn system_ram_gb() -> u32 {
    let mut sys = System::new();
    sys.refresh_memory();
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    (sys.total_memory() as f64 / GB).round() as u32
}

/// Clef-Flash unless the machine is below its minimum RAM; then Kev-4B.
/// Clef 27B is opt-in only and never recommended.
pub fn recommended(total_ram_gb: u32) -> &'static ModelSpec {
    let flash = &registry::REGISTRY[0];
    if total_ram_gb < flash.min_ram_gb {
        &registry::REGISTRY[2]
    } else {
        flash
    }
}

/// Where hf-hub places (and finds) a spec's file under `dir`.
pub fn model_path(spec: &ModelSpec, dir: &Path) -> Option<PathBuf> {
    Cache::new(dir.to_path_buf())
        .model(spec.hf_repo.to_string())
        .get(spec.hf_file)
}

pub fn installed(dir: &Path) -> Vec<&'static ModelSpec> {
    REGISTRY
        .iter()
        .filter(|spec| {
            model_path(spec, dir)
                .and_then(|p| std::fs::metadata(p).ok())
                .is_some_and(|m| m.len() == spec.size_bytes)
        })
        .collect()
}

/// Streams the file through SHA-256 (models are many GB).
pub fn verify_sha256(path: &Path, expected: &str) -> Result<(), ModelError> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = hex(&hasher.finalize());
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(ModelError::ChecksumMismatch)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Clone)]
struct ProgressAdapter {
    cb: Arc<dyn Fn(u64, u64) + Send + Sync>,
    done: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
}

impl Progress for ProgressAdapter {
    async fn init(&mut self, size: usize, _filename: &str) {
        self.total.store(size as u64, Ordering::Relaxed);
        self.done.store(0, Ordering::Relaxed);
        (self.cb)(0, size as u64);
    }

    async fn update(&mut self, size: usize) {
        let done = self.done.fetch_add(size as u64, Ordering::Relaxed) + size as u64;
        (self.cb)(done, self.total.load(Ordering::Relaxed));
    }

    async fn finish(&mut self) {}
}

/// Downloads `spec` into hf-hub's cache layout under `dir` (resuming a
/// partial download), then verifies its SHA-256. A corrupt file is deleted.
pub async fn download(
    spec: &ModelSpec,
    dir: &Path,
    on_progress: impl Fn(u64, u64) + Send + Sync + 'static,
) -> Result<PathBuf, ModelError> {
    let api = ApiBuilder::new()
        .with_cache_dir(dir.to_path_buf())
        .with_progress(false)
        .build()
        .map_err(|e| ModelError::Network(e.to_string()))?;
    let progress = ProgressAdapter {
        cb: Arc::new(on_progress),
        done: Arc::new(AtomicU64::new(0)),
        total: Arc::new(AtomicU64::new(0)),
    };
    let path = api
        .model(spec.hf_repo.to_string())
        .download_with_progress(spec.hf_file, progress)
        .await
        .map_err(|e| ModelError::Network(e.to_string()))?;

    let (p, expected) = (path.clone(), spec.sha256);
    let verified = tokio::task::spawn_blocking(move || verify_sha256(&p, expected))
        .await
        .map_err(|e| ModelError::Io(std::io::Error::other(e)))?;
    if let Err(e) = verified {
        if matches!(e, ModelError::ChecksumMismatch) {
            // `path` is a symlink into blobs/; remove the blob and the link.
            if let Ok(blob) = std::fs::canonicalize(&path) {
                let _ = std::fs::remove_file(blob);
            }
            let _ = std::fs::remove_file(&path);
        }
        return Err(e);
    }
    Ok(path)
}
