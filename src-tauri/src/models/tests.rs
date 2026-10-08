use super::*;
use std::fs;

#[test]
fn recommends_clef_flash_at_16gb() {
    assert_eq!(recommended(16).id, "clef-flash");
}

#[test]
fn recommends_fallback_at_6gb() {
    assert_eq!(recommended(6).id, "kev-4b");
}

#[test]
fn recommends_27b_never_by_default() {
    assert_eq!(recommended(64).id, "clef-flash");
}

#[test]
fn verify_rejects_bad_checksum() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.gguf");
    fs::write(&path, b"hello").unwrap();
    let wrong = "0".repeat(64);
    assert!(matches!(
        verify_sha256(&path, &wrong),
        Err(ModelError::ChecksumMismatch)
    ));
    // sha256("hello")
    let right = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    assert!(verify_sha256(&path, right).is_ok());
}

#[test]
fn installed_requires_size_match() {
    let dir = tempfile::tempdir().unwrap();
    let spec = &registry::KEV_4B;
    // Lay out a truncated file where hf-hub's cache puts it.
    let repo = dir.path().join("models--ggml-org--Kev-4B-GGUF");
    let snap = repo.join("snapshots").join("abc123");
    fs::create_dir_all(&snap).unwrap();
    fs::create_dir_all(repo.join("refs")).unwrap();
    fs::write(repo.join("refs").join("main"), "abc123").unwrap();
    fs::write(snap.join(spec.hf_file), b"truncated").unwrap();

    assert!(model_path(spec, dir.path()).is_some());
    assert!(installed(dir.path()).is_empty());
}
