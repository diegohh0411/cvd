//! `#[tauri::command]` wrappers; names are the IPC contract.
use tauri::State;

use cvd_core::services::*;
use cvd_core::{
    autofill::{ApplyDefaults, Fact},
    mail::watch::WatcherStatus,
    matcher::Ranked,
    providers::ProviderModels,
    render::fit::{Override, Selection},
    state::AppState,
    store::{types::*, InboxFilter},
};
type Res<T> = Result<T, AppError>;

type S<'a> = State<'a, std::sync::Arc<AppState>>;

#[tauri::command]
pub async fn get_local_connection(st: S<'_>) -> Res<LocalConnectionView> {
    cvd_core::services::get_local_connection(&st).await
}
#[tauri::command]
pub async fn save_local_connection(st: S<'_>, base_url: String, key: Option<String>) -> Res<()> {
    cvd_core::services::save_local_connection(&st, &base_url, key).await
}
#[tauri::command]
pub async fn test_local_connection(st: S<'_>, model: String, role: String) -> Res<()> {
    cvd_core::services::test_local_connection(&st, &model, &role).await
}
#[tauri::command]
pub async fn get_profile(st: S<'_>) -> Res<Profile> {
    cvd_core::services::get_profile(&st).await
}
#[tauri::command]
pub async fn save_profile(st: S<'_>, profile: Profile) -> Res<()> {
    cvd_core::services::save_profile(&st, profile).await
}
#[tauri::command]
pub async fn list_entries(st: S<'_>, include_archived: bool) -> Res<Vec<Entry>> {
    cvd_core::services::list_entries(&st, include_archived).await
}
#[tauri::command]
pub async fn save_entry(st: S<'_>, input: EntryInput) -> Res<i64> {
    cvd_core::services::save_entry(&st, input).await
}
#[tauri::command]
pub async fn delete_entry(st: S<'_>, id: i64) -> Res<()> {
    cvd_core::services::delete_entry(&st, id).await
}
#[tauri::command]
pub async fn list_skills(st: S<'_>) -> Res<Vec<Skill>> {
    cvd_core::services::list_skills(&st).await
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
    cvd_core::services::create_job(&st, &title, &company, &raw_text, &url, status_id).await
}
#[tauri::command]
pub async fn update_job_tracking(
    st: S<'_>,
    id: i64,
    status_id: Option<i64>,
    url: String,
) -> Res<()> {
    cvd_core::services::update_job_tracking(&st, id, status_id, &url).await
}
#[tauri::command]
pub async fn update_job_description(st: S<'_>, id: i64, raw_text: String) -> Res<()> {
    cvd_core::services::update_job_description(&st, id, &raw_text).await
}
#[tauri::command]
pub async fn fetch_job_description(url: String) -> Res<String> {
    cvd_core::services::fetch_job_description(&url).await
}
#[tauri::command]
pub async fn list_job_views(st: S<'_>) -> Res<Vec<JobView>> {
    cvd_core::services::list_job_views(&st).await
}
#[tauri::command]
pub async fn create_job_view(st: S<'_>, view: JobViewInput) -> Res<JobView> {
    cvd_core::services::create_job_view(&st, &view).await
}
#[tauri::command]
pub async fn update_job_view(st: S<'_>, id: i64, view: JobViewInput) -> Res<JobView> {
    cvd_core::services::update_job_view(&st, id, &view).await
}
#[tauri::command]
pub async fn delete_job_view(st: S<'_>, id: i64) -> Res<()> {
    cvd_core::services::delete_job_view(&st, id).await
}
#[tauri::command]
pub async fn list_job_statuses(st: S<'_>) -> Res<Vec<JobStatus>> {
    cvd_core::services::list_job_statuses(&st).await
}
#[tauri::command]
pub async fn create_job_status(st: S<'_>, name: String, color: StatusColor) -> Res<JobStatus> {
    cvd_core::services::create_job_status(&st, &name, color).await
}
#[tauri::command]
pub async fn update_job_status(
    st: S<'_>,
    id: i64,
    name: String,
    color: StatusColor,
) -> Res<JobStatus> {
    cvd_core::services::update_job_status(&st, id, &name, color).await
}
#[tauri::command]
pub async fn delete_job_status(st: S<'_>, id: i64) -> Res<()> {
    cvd_core::services::delete_job_status(&st, id).await
}
#[tauri::command]
pub async fn reorder_job_statuses(st: S<'_>, ids: Vec<i64>) -> Res<()> {
    cvd_core::services::reorder_job_statuses(&st, ids).await
}
#[tauri::command]
pub async fn list_jobs(st: S<'_>) -> Res<Vec<JobDescription>> {
    cvd_core::services::list_jobs(&st).await
}
#[tauri::command]
pub async fn search_jobs(st: S<'_>, query: String) -> Res<Vec<i64>> {
    cvd_core::services::search_jobs(&st, &query).await
}
#[tauri::command]
pub async fn get_job(st: S<'_>, id: i64) -> Res<JobDescription> {
    cvd_core::services::get_job(&st, id).await
}
#[tauri::command]
pub async fn start_match(st: S<'_>, jd_id: i64) -> Res<i64> {
    cvd_core::services::start_match(&st, jd_id).await
}
#[tauri::command]
pub fn cancel_match(st: S<'_>, run_id: i64) -> Res<()> {
    cvd_core::services::cancel_match(&st, run_id)
}
#[tauri::command]
pub async fn list_runs(st: S<'_>, jd_id: i64) -> Res<Vec<MatchRun>> {
    cvd_core::services::list_runs(&st, jd_id).await
}
#[tauri::command]
pub async fn get_scores(st: S<'_>, run_id: i64) -> Res<Vec<ScoreRow>> {
    cvd_core::services::get_scores(&st, run_id).await
}
#[tauri::command]
pub async fn list_cvs(st: S<'_>, jd_id: i64) -> Res<Vec<Cv>> {
    cvd_core::services::list_cvs(&st, jd_id).await
}
#[tauri::command]
pub async fn set_provider_key(st: S<'_>, provider: String, key: String) -> Res<KeyStatus> {
    cvd_core::services::set_provider_key(&st, &provider, &key).await
}
#[tauri::command]
pub async fn delete_provider_key(st: S<'_>, provider: String) -> Res<()> {
    cvd_core::services::delete_provider_key(&st, &provider).await
}
#[tauri::command]
pub async fn list_connections(st: S<'_>) -> Res<Vec<ConnectionView>> {
    cvd_core::services::list_connections(&st).await
}
#[tauri::command]
pub async fn add_connection(
    st: S<'_>,
    provider: String,
    label: String,
    key: String,
) -> Res<ConnectionView> {
    cvd_core::services::add_connection(&st, &provider, &label, &key).await
}
#[tauri::command]
pub async fn rename_connection(st: S<'_>, id: String, label: String) -> Res<()> {
    cvd_core::services::rename_connection(&st, &id, &label).await
}
#[tauri::command]
pub async fn provider_key_status(st: S<'_>, provider: String) -> Res<KeyStatus> {
    cvd_core::services::provider_key_status(&st, &provider).await
}
#[tauri::command]
pub async fn list_remote_models(
    st: S<'_>,
    refresh: bool,
    role: Option<String>,
) -> Res<Vec<ProviderModels>> {
    cvd_core::services::list_remote_models(&st, refresh, role.as_deref().unwrap_or("decision"))
        .await
}
#[tauri::command]
pub async fn get_apply_defaults(st: S<'_>) -> Res<ApplyDefaults> {
    cvd_core::services::get_apply_defaults(&st).await
}
#[tauri::command]
pub async fn save_apply_defaults(st: S<'_>, defaults: ApplyDefaults) -> Res<()> {
    cvd_core::services::save_apply_defaults(&st, defaults).await
}
#[tauri::command]
pub async fn apply_facts(st: S<'_>) -> Res<Vec<Fact>> {
    cvd_core::services::apply_facts(&st).await
}
#[tauri::command]
pub async fn start_apply(app: tauri::AppHandle, st: S<'_>, job_id: i64) -> Res<bool> {
    crate::autofill::window::start_apply(&app, &st, job_id).await
}
#[tauri::command]
pub async fn get_ui_setting(st: S<'_>, key: String) -> Res<Option<String>> {
    cvd_core::services::get_ui_setting(&st, &key).await
}
#[tauri::command]
pub async fn set_ui_setting(st: S<'_>, key: String, value: String) -> Res<()> {
    cvd_core::services::set_ui_setting(&st, &key, &value).await
}
#[tauri::command]
pub async fn engine_status(st: S<'_>) -> Res<crate::state::EngineStatus> {
    cvd_core::services::engine_status(&st).await
}
#[tauri::command]
pub async fn get_ranked(st: S<'_>, run_id: i64) -> Res<Ranked> {
    cvd_core::services::get_ranked(&st, run_id).await
}
#[tauri::command]
pub async fn fit_cv(
    st: S<'_>,
    run_id: i64,
    budget: u8,
    overrides: Vec<Override>,
) -> Res<FitOutput> {
    cvd_core::services::fit_cv(&st, run_id, budget, overrides).await
}
#[tauri::command]
pub async fn export_cv(st: S<'_>, run_id: i64, selection: Selection, path: String) -> Res<i64> {
    cvd_core::services::export_cv(&st, run_id, selection, path).await
}
#[tauri::command]
pub async fn set_active_model(st: S<'_>, id: String) -> Res<()> {
    cvd_core::services::set_active_model(&st, &id).await
}
#[tauri::command]
pub async fn skip_research(st: S<'_>, run_id: i64, mode: String) -> Res<()> {
    cvd_core::services::skip_research(&st, run_id, &mode).await
}
#[tauri::command]
pub async fn rescore(st: S<'_>, run_id: i64) -> Res<i64> {
    cvd_core::services::rescore(&st, run_id).await
}
#[tauri::command]
pub async fn get_company(st: S<'_>, id: i64) -> Res<Company> {
    cvd_core::services::get_company(&st, id).await
}
#[tauri::command]
pub async fn get_job_company(st: S<'_>, jd_id: i64) -> Res<Option<Company>> {
    cvd_core::services::get_job_company(&st, jd_id).await
}
#[tauri::command]
pub async fn edit_company(st: S<'_>, id: i64, profile: CompanyProfile) -> Res<Company> {
    cvd_core::services::edit_company(&st, id, profile).await
}
#[tauri::command]
pub async fn refresh_company(st: S<'_>, id: i64) -> Res<Company> {
    cvd_core::services::refresh_company(&st, id).await
}
#[tauri::command]
pub async fn set_research_model(st: S<'_>, id: String) -> Res<()> {
    cvd_core::services::set_research_model(&st, &id).await
}
#[tauri::command]
pub async fn get_research_model(st: S<'_>) -> Res<Option<String>> {
    st.research_model().await
}
#[tauri::command]
pub async fn get_mail_account(st: S<'_>) -> Res<Option<MailAccountView>> {
    cvd_core::services::get_mail_account(&st).await
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
    cvd_core::services::save_mail_account(
        &st,
        &host,
        port,
        &address,
        password.as_deref(),
        folder.as_deref(),
    )
    .await
}
#[tauri::command]
pub async fn delete_mail_account(st: S<'_>) -> Res<()> {
    cvd_core::services::delete_mail_account(&st).await
}
#[tauri::command]
pub async fn test_mail_account(
    st: S<'_>,
    host: String,
    port: u16,
    address: String,
    password: Option<String>,
) -> Res<Vec<String>> {
    cvd_core::services::test_mail_account(&st, &host, port, &address, password.as_deref()).await
}
#[tauri::command]
pub async fn start_scan(st: S<'_>) -> Res<i64> {
    cvd_core::services::start_scan(&st).await
}
#[tauri::command]
pub async fn get_scan_status(st: S<'_>) -> Res<ScanState> {
    Ok(cvd_core::services::get_scan_status(&st))
}
#[tauri::command]
pub async fn list_job_emails(st: S<'_>, job_id: i64) -> Res<Vec<ProcessedEmailView>> {
    cvd_core::services::list_job_emails(&st, job_id).await
}
#[tauri::command]
pub async fn list_pending_reviews(st: S<'_>) -> Res<Vec<ProcessedEmailView>> {
    cvd_core::services::list_pending_reviews(&st).await
}
#[tauri::command]
pub async fn apply_suggestion(st: S<'_>, message_id: String) -> Res<ProcessedEmailView> {
    cvd_core::services::apply_suggestion(&st, &message_id).await
}
#[tauri::command]
pub async fn dismiss_suggestion(st: S<'_>, message_id: String) -> Res<()> {
    cvd_core::services::dismiss_suggestion(&st, &message_id).await
}
#[tauri::command]
pub async fn get_watcher_status(st: S<'_>) -> Res<WatcherStatus> {
    Ok(cvd_core::services::get_watcher_status(&st))
}
#[tauri::command]
pub async fn retry_watcher(st: S<'_>) -> Res<()> {
    cvd_core::services::retry_watcher(&st);
    Ok(())
}
#[tauri::command]
pub async fn list_inbox(st: S<'_>, filter: InboxFilter) -> Res<Vec<ProcessedEmailView>> {
    cvd_core::services::list_inbox(&st, filter).await
}
#[tauri::command]
pub async fn unread_count(st: S<'_>) -> Res<i64> {
    cvd_core::services::unread_count(&st).await
}
#[tauri::command]
pub async fn mark_read(st: S<'_>, message_id: String) -> Res<()> {
    cvd_core::services::mark_read(&st, &message_id).await
}
#[tauri::command]
pub async fn mark_all_read(st: S<'_>) -> Res<()> {
    cvd_core::services::mark_all_read(&st).await
}
#[tauri::command]
pub async fn undo_applied(st: S<'_>, message_id: String) -> Res<ProcessedEmailView> {
    cvd_core::services::undo_applied(&st, &message_id).await
}
