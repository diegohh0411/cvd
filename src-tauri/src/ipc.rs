//! `#[tauri::command]` wrappers; names are the IPC contract.
use tauri::State;

use cvd_core::commands::{self as cmd, *};
use cvd_core::state::{AppState, EngineStatus};
use cvd_core::store::types::*;

type S<'a> = State<'a, AppState>;

#[tauri::command]
pub async fn get_profile(st: S<'_>) -> Res<Profile> {
    cmd::get_profile(&st).await
}
#[tauri::command]
pub async fn save_profile(st: S<'_>, profile: Profile) -> Res<()> {
    cmd::save_profile(&st, profile).await
}
#[tauri::command]
pub async fn list_entries(st: S<'_>, include_archived: bool) -> Res<Vec<Entry>> {
    cmd::list_entries(&st, include_archived).await
}
#[tauri::command]
pub async fn save_entry(st: S<'_>, input: EntryInput) -> Res<i64> {
    cmd::save_entry(&st, input).await
}
#[tauri::command]
pub async fn delete_entry(st: S<'_>, id: i64) -> Res<()> {
    cmd::delete_entry(&st, id).await
}
#[tauri::command]
pub async fn list_skills(st: S<'_>) -> Res<Vec<Skill>> {
    cmd::list_skills(&st).await
}
#[tauri::command]
pub async fn create_job(
    st: S<'_>,
    title: String,
    company: String,
    raw_text: String,
    url: String,
    status_id: Option<i64>,
) -> Res<i64> {
    cmd::create_job(&st, &title, &company, &raw_text, &url, status_id).await
}
#[tauri::command]
pub async fn update_job_tracking(st: S<'_>, id: i64, status_id: Option<i64>, url: String) -> Res<()> {
    cmd::update_job_tracking(&st, id, status_id, &url).await
}
#[tauri::command]
pub async fn update_job_description(st: S<'_>, id: i64, raw_text: String) -> Res<()> {
    cmd::update_job_description(&st, id, &raw_text).await
}
#[tauri::command]
pub async fn fetch_job_description(url: String) -> Res<String> {
    cmd::fetch_job_description(&url).await
}
#[tauri::command]
pub async fn list_job_statuses(st: S<'_>) -> Res<Vec<JobStatus>> {
    cmd::list_job_statuses(&st).await
}
#[tauri::command]
pub async fn create_job_status(st: S<'_>, name: String, color: StatusColor) -> Res<JobStatus> {
    cmd::create_job_status(&st, &name, color).await
}
#[tauri::command]
pub async fn update_job_status(st: S<'_>, id: i64, name: String, color: StatusColor) -> Res<JobStatus> {
    cmd::update_job_status(&st, id, &name, color).await
}
#[tauri::command]
pub async fn delete_job_status(st: S<'_>, id: i64) -> Res<()> {
    cmd::delete_job_status(&st, id).await
}
#[tauri::command]
pub async fn reorder_job_statuses(st: S<'_>, ids: Vec<i64>) -> Res<()> {
    cmd::reorder_job_statuses(&st, ids).await
}
#[tauri::command]
pub async fn list_jobs(st: S<'_>) -> Res<Vec<JobDescription>> {
    cmd::list_jobs(&st).await
}
#[tauri::command]
pub async fn get_job(st: S<'_>, id: i64) -> Res<JobDescription> {
    cmd::get_job(&st, id).await
}
#[tauri::command]
pub async fn start_match(st: S<'_>, jd_id: i64) -> Res<i64> {
    cmd::start_match(&st, jd_id).await
}
#[tauri::command]
pub fn cancel_match(st: S<'_>, run_id: i64) -> Res<()> {
    cmd::cancel_match(&st, run_id)
}
#[tauri::command]
pub async fn resume_match(st: S<'_>, run_id: i64) -> Res<()> {
    cmd::resume_match(&st, run_id).await
}
#[tauri::command]
pub async fn list_runs(st: S<'_>, jd_id: i64) -> Res<Vec<MatchRun>> {
    cmd::list_runs(&st, jd_id).await
}
#[tauri::command]
pub async fn get_scores(st: S<'_>, run_id: i64) -> Res<Vec<ScoreRow>> {
    cmd::get_scores(&st, run_id).await
}
#[tauri::command]
pub async fn list_cvs(st: S<'_>, jd_id: i64) -> Res<Vec<Cv>> {
    cmd::list_cvs(&st, jd_id).await
}
#[tauri::command]
pub async fn set_provider_key(st: S<'_>, provider: String, key: String) -> Res<KeyStatus> {
    cmd::set_provider_key(&st, &provider, &key).await
}
#[tauri::command]
pub async fn delete_provider_key(st: S<'_>, provider: String) -> Res<()> {
    cmd::delete_provider_key(&st, &provider).await
}
#[tauri::command]
pub async fn list_connections(st: S<'_>) -> Res<Vec<ConnectionView>> {
    cmd::list_connections(&st).await
}
#[tauri::command]
pub async fn add_connection(st: S<'_>, provider: String, label: String, key: String) -> Res<ConnectionView> {
    cmd::add_connection(&st, &provider, &label, &key).await
}
#[tauri::command]
pub async fn rename_connection(st: S<'_>, id: String, label: String) -> Res<()> {
    cmd::rename_connection(&st, &id, &label).await
}
#[tauri::command]
pub async fn provider_key_status(st: S<'_>, provider: String) -> Res<KeyStatus> {
    cmd::provider_key_status(&st, &provider).await
}
#[tauri::command]
pub async fn list_remote_models(st: S<'_>, refresh: bool, role: Option<String>) -> Res<Vec<ProviderModels>> {
    cmd::list_remote_models(&st, refresh, role.as_deref().unwrap_or("decision")).await
}
#[tauri::command]
pub async fn get_ui_setting(st: S<'_>, key: String) -> Res<Option<String>> {
    cmd::get_ui_setting(&st, &key).await
}
#[tauri::command]
pub async fn set_ui_setting(st: S<'_>, key: String, value: String) -> Res<()> {
    cmd::set_ui_setting(&st, &key, &value).await
}
#[tauri::command]
pub async fn engine_status(st: S<'_>) -> Res<EngineStatus> {
    cmd::engine_status(&st).await
}
#[tauri::command]
pub fn cancel_download(st: S<'_>, id: String) -> Res<()> {
    cmd::cancel_download(&st, &id)
}
#[tauri::command]
pub async fn get_ranked(st: S<'_>, run_id: i64) -> Res<Ranked> {
    cmd::get_ranked(&st, run_id).await
}
#[tauri::command]
pub async fn fit_cv(st: S<'_>, run_id: i64, budget: u8, overrides: Vec<Override>) -> Res<FitOutput> {
    cmd::fit_cv(&st, run_id, budget, overrides).await
}
#[tauri::command]
pub async fn export_cv(st: S<'_>, run_id: i64, selection: Selection, path: String) -> Res<i64> {
    cmd::export_cv(&st, run_id, selection, path).await
}
#[tauri::command]
pub fn system_ram_gb() -> u32 {
    cvd_core::models::system_ram_gb()
}
#[tauri::command]
pub fn list_models(st: S<'_>) -> Vec<ModelInfo> {
    cmd::list_models(&st)
}
#[tauri::command]
pub async fn download_model(st: S<'_>, id: String) -> Res<()> {
    cmd::download_model(&st, &id).await
}
#[tauri::command]
pub async fn set_active_model(st: S<'_>, id: String) -> Res<()> {
    cmd::set_active_model(&st, &id).await
}
#[tauri::command]
pub async fn skip_research(st: S<'_>, run_id: i64, mode: String) -> Res<()> {
    cmd::skip_research(&st, run_id, &mode).await
}
#[tauri::command]
pub async fn rescore(st: S<'_>, run_id: i64) -> Res<i64> {
    cmd::rescore(&st, run_id).await
}
#[tauri::command]
pub async fn get_company(st: S<'_>, id: i64) -> Res<Company> {
    cmd::get_company(&st, id).await
}
#[tauri::command]
pub async fn get_job_company(st: S<'_>, jd_id: i64) -> Res<Option<Company>> {
    cmd::get_job_company(&st, jd_id).await
}
#[tauri::command]
pub async fn edit_company(st: S<'_>, id: i64, profile: CompanyProfile) -> Res<Company> {
    cmd::edit_company(&st, id, profile).await
}
#[tauri::command]
pub async fn refresh_company(st: S<'_>, id: i64) -> Res<Company> {
    cmd::refresh_company(&st, id).await
}
#[tauri::command]
pub async fn set_research_model(st: S<'_>, id: String) -> Res<()> {
    cmd::set_research_model(&st, &id).await
}
#[tauri::command]
pub async fn get_research_model(st: S<'_>) -> Res<Option<String>> {
    st.research_model().await
}
#[tauri::command]
pub async fn get_mail_account(st: S<'_>) -> Res<Option<MailAccountView>> {
    cmd::get_mail_account(&st).await
}
#[tauri::command]
pub async fn save_mail_account(
    st: S<'_>,
    host: String,
    port: u16,
    address: String,
    password: Option<String>,
    folder: Option<String>,
) -> Res<MailAccountView> {
    cmd::save_mail_account(&st, &host, port, &address, password.as_deref(), folder.as_deref()).await
}
#[tauri::command]
pub async fn delete_mail_account(st: S<'_>) -> Res<()> {
    cmd::delete_mail_account(&st).await
}
#[tauri::command]
pub async fn test_mail_account(
    st: S<'_>,
    host: String,
    port: u16,
    address: String,
    password: Option<String>,
) -> Res<Vec<String>> {
    cmd::test_mail_account(&st, &host, port, &address, password.as_deref()).await
}
#[tauri::command]
pub async fn start_scan(st: S<'_>) -> Res<i64> {
    cmd::start_scan(&st).await
}
#[tauri::command]
pub async fn get_scan_status(st: S<'_>) -> Res<ScanState> {
    Ok(cmd::get_scan_status(&st))
}
#[tauri::command]
pub async fn list_scan_results(st: S<'_>, run_id: i64) -> Res<Vec<ProcessedEmailView>> {
    cmd::list_scan_results(&st, run_id).await
}
#[tauri::command]
pub async fn list_pending_reviews(st: S<'_>) -> Res<Vec<ProcessedEmailView>> {
    cmd::list_pending_reviews(&st).await
}
#[tauri::command]
pub async fn apply_suggestion(st: S<'_>, message_id: String) -> Res<ProcessedEmailView> {
    cmd::apply_suggestion(&st, &message_id).await
}
#[tauri::command]
pub async fn dismiss_suggestion(st: S<'_>, message_id: String) -> Res<()> {
    cmd::dismiss_suggestion(&st, &message_id).await
}
