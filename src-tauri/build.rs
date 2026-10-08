fn main() {
    copy_llama_dylibs();
    tauri_build::build()
}

/// Dev builds run the `llama-server` sidecar straight from `target/<profile>`,
/// where bundling (`macOS.files`) never puts its dylibs. Copy them alongside so
/// its `@loader_path` rpath resolves.
fn copy_llama_dylibs() {
    use std::path::PathBuf;
    println!("cargo:rerun-if-changed=binaries");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    // OUT_DIR is target/<profile>/build/<pkg>/out.
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let Some(profile_dir) = out.ancestors().nth(3) else { return };
    let Ok(entries) = std::fs::read_dir("binaries") else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "dylib") {
            let _ = std::fs::copy(&path, profile_dir.join(entry.file_name()));
        }
    }
}
