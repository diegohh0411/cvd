// The command registry: `SHARED`, `DESKTOP_ONLY` and `PER_TRANSPORT`.
include!("../../../crates/cvd-core/src/commands.rs");

fn main() {
    println!("cargo:rerun-if-changed=../../../crates/cvd-core/src/commands.rs");
    // Declaring an app manifest puts all app commands under the ACL, so each one
    // needs a permission or the webview is denied.
    let commands: &'static [&str] = SHARED
        .iter()
        .map(|(name, _)| *name)
        .chain(DESKTOP_ONLY.iter().copied())
        .chain(PER_TRANSPORT.iter().copied())
        .collect::<Vec<_>>()
        .leak();
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(commands)),
    )
    .expect("failed to run tauri-build");
}
