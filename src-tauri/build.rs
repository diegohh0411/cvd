/// Every command in `generate_handler!`. Declaring an app manifest puts all app
/// commands under the ACL, so each one needs a permission or the webview is denied.
const COMMANDS: &[&str] = &[
    "get_profile",
    "save_profile",
    "list_entries",
    "save_entry",
    "delete_entry",
    "list_skills",
    "create_job",
    "update_job_tracking",
    "update_job_description",
    "fetch_job_description",
    "list_job_statuses",
    "create_job_status",
    "update_job_status",
    "delete_job_status",
    "reorder_job_statuses",
    "list_jobs",
    "search_jobs",
    "get_job",
    "start_match",
    "cancel_match",
    "resume_match",
    "list_runs",
    "get_scores",
    "list_cvs",
    "get_ui_setting",
    "set_ui_setting",
    "get_apply_defaults",
    "save_apply_defaults",
    "apply_facts",
    "engine_status",
    "cancel_download",
    "get_ranked",
    "fit_cv",
    "export_cv",
    "system_ram_gb",
    "list_models",
    "download_model",
    "set_active_model",
    "set_research_model",
    "skip_research",
    "rescore",
    "get_company",
    "get_job_company",
    "edit_company",
    "refresh_company",
    "get_research_model",
    "get_mail_account",
    "save_mail_account",
    "delete_mail_account",
    "test_mail_account",
    "start_scan",
    "get_scan_status",
    "list_scan_results",
    "list_pending_reviews",
    "list_job_emails",
    "apply_suggestion",
    "dismiss_suggestion",
    "set_provider_key",
    "delete_provider_key",
    "provider_key_status",
    "list_connections",
    "add_connection",
    "rename_connection",
    "list_remote_models",
    "start_apply",
    "autofill_bridge",
];

fn main() {
    copy_llama_dylibs();
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
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
