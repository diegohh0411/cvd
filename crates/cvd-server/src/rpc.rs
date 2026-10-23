//! `POST /api/rpc/:command`: the same names and camelCase args as the Tauri commands.
use serde::de::DeserializeOwned;
use serde_json::Value;

use cvd_core::services::{self, *};
use cvd_core::{
    autofill::ApplyDefaults,
    render::fit::{Override, Selection},
    state::AppState,
    store::{types::*, InboxFilter},
};

/// Desktop-only commands; the web build hides their UI.
pub const DESKTOP_ONLY: &[&str] = &[
    "get_local_connection",
    "save_local_connection",
    "test_local_connection",
    "save_file_bytes",
    "start_apply",
    "autofill_bridge",
];

/// Why a call failed; `Unknown` maps to 404, everything else keeps the `AppError` body.
#[derive(Debug, PartialEq)]
pub enum RpcError {
    Unknown(String),
    App(AppError),
}
impl From<AppError> for RpcError {
    fn from(e: AppError) -> Self {
        Self::App(e)
    }
}

fn args<T: DeserializeOwned>(cmd: &str, v: Value) -> Result<T, RpcError> {
    // Tauri accepts a missing args object for commands without required args.
    let v = if v.is_null() {
        Value::Object(Default::default())
    } else {
        v
    };
    serde_json::from_value(v)
        .map_err(|e| AppError::Io(format!("invalid arguments for {cmd}: {e}")).into())
}

fn json<T: serde::Serialize>(v: T) -> Result<Value, RpcError> {
    serde_json::to_value(v).map_err(|e| AppError::Io(e.to_string()).into())
}

/// One arm per command: `name(arg: Type, ..) => body`, where `body` is a `Res<T>`
/// expression that may `.await` and sees the state as `st`.
macro_rules! commands {
    ($st:ident; $($name:ident ($($arg:ident : $ty:ty),* $(,)?) => $body:expr;)*) => {
        /// Every command the server dispatches, for tests and the desktop-parity check.
        #[cfg_attr(not(test), allow(dead_code))]
        pub const COMMANDS: &[&str] = &[$(stringify!($name)),*];

        pub async fn dispatch($st: &AppState, cmd: &str, raw: Value) -> Result<Value, RpcError> {
            match cmd {
                $(stringify!($name) => {
                    #[derive(serde::Deserialize)]
                    #[serde(rename_all = "camelCase")]
                    #[allow(dead_code)]
                    struct Args { $($arg: $ty),* }
                    #[allow(unused_variables)]
                    let Args { $($arg),* } = args::<Args>(cmd, raw)?;
                    json($body?)
                })*
                _ if DESKTOP_ONLY.contains(&cmd) => Err(AppError::Io(format!(
                    "{cmd} is not available on the web; use the desktop app."
                ))
                .into()),
                _ => Err(RpcError::Unknown(cmd.to_owned())),
            }
        }
    };
}

commands! {
    st;
    get_profile() => get_profile(st).await;
    save_profile(profile: Profile) => save_profile(st, profile).await;
    list_entries(include_archived: bool) => list_entries(st, include_archived).await;
    save_entry(input: EntryInput) => save_entry(st, input).await;
    delete_entry(id: i64) => delete_entry(st, id).await;
    list_skills() => list_skills(st).await;
    create_job(title: String, company: String, raw_text: String, url: String, status_id: Option<i64>) =>
        create_job(st, &title, &company, &raw_text, &url, status_id).await;
    update_job_tracking(id: i64, status_id: Option<i64>, url: String) =>
        update_job_tracking(st, id, status_id, &url).await;
    update_job_description(id: i64, raw_text: String) => update_job_description(st, id, &raw_text).await;
    fetch_job_description(url: String) => fetch_job_description(&url).await;
    list_job_statuses() => list_job_statuses(st).await;
    create_job_status(name: String, color: StatusColor) => create_job_status(st, &name, color).await;
    update_job_status(id: i64, name: String, color: StatusColor) =>
        update_job_status(st, id, &name, color).await;
    delete_job_status(id: i64) => delete_job_status(st, id).await;
    reorder_job_statuses(ids: Vec<i64>) => reorder_job_statuses(st, ids).await;
    list_job_views() => list_job_views(st).await;
    create_job_view(view: JobViewInput) => create_job_view(st, &view).await;
    update_job_view(id: i64, view: JobViewInput) => update_job_view(st, id, &view).await;
    delete_job_view(id: i64) => delete_job_view(st, id).await;
    list_jobs() => list_jobs(st).await;
    search_jobs(query: String) => search_jobs(st, &query).await;
    get_job(id: i64) => get_job(st, id).await;
    start_match(jd_id: i64) => start_match(st, jd_id).await;
    cancel_match(run_id: i64) => cancel_match(st, run_id);
    list_runs(jd_id: i64) => list_runs(st, jd_id).await;
    get_scores(run_id: i64) => get_scores(st, run_id).await;
    list_cvs(jd_id: i64) => list_cvs(st, jd_id).await;
    get_ui_setting(key: String) => get_ui_setting(st, &key).await;
    set_ui_setting(key: String, value: String) => set_ui_setting(st, &key, &value).await;
    get_apply_defaults() => get_apply_defaults(st).await;
    save_apply_defaults(defaults: ApplyDefaults) => save_apply_defaults(st, defaults).await;
    apply_facts() => apply_facts(st).await;
    engine_status() => engine_status(st).await;
    get_ranked(run_id: i64) => get_ranked(st, run_id).await;
    fit_cv(run_id: i64, budget: u8, overrides: Vec<Override>) => fit_cv(st, run_id, budget, overrides).await;
    // The browser downloads the bytes; there is no server-side file to record.
    export_cv(run_id: i64, selection: Selection) => services::export_cv(st, run_id, selection).await;
    set_active_model(id: String) => set_active_model(st, &id).await;
    set_research_model(id: String) => set_research_model(st, &id).await;
    skip_research(run_id: i64, mode: String) => skip_research(st, run_id, &mode).await;
    rescore(run_id: i64) => rescore(st, run_id).await;
    get_company(id: i64) => get_company(st, id).await;
    get_job_company(jd_id: i64) => get_job_company(st, jd_id).await;
    edit_company(id: i64, profile: CompanyProfile) => edit_company(st, id, profile).await;
    refresh_company(id: i64) => refresh_company(st, id).await;
    get_research_model() => st.research_model().await;
    get_mail_account() => get_mail_account(st).await;
    save_mail_account(host: String, port: u16, address: String, password: Option<String>, folder: Option<String>) =>
        save_mail_account(st, &host, port, &address, password.as_deref(), folder.as_deref()).await;
    delete_mail_account() => delete_mail_account(st).await;
    test_mail_account(host: String, port: u16, address: String, password: Option<String>) =>
        test_mail_account(st, &host, port, &address, password.as_deref()).await;
    start_scan() => start_scan(st).await;
    get_scan_status() => Ok::<_, AppError>(get_scan_status(st));
    list_pending_reviews() => list_pending_reviews(st).await;
    list_job_emails(job_id: i64) => list_job_emails(st, job_id).await;
    apply_suggestion(message_id: String) => apply_suggestion(st, &message_id).await;
    dismiss_suggestion(message_id: String) => dismiss_suggestion(st, &message_id).await;
    get_watcher_status() => Ok::<_, AppError>(get_watcher_status(st));
    retry_watcher() => Ok::<_, AppError>(retry_watcher(st));
    list_inbox(filter: InboxFilter) => list_inbox(st, filter).await;
    unread_count() => unread_count(st).await;
    mark_read(message_id: String) => mark_read(st, &message_id).await;
    mark_all_read() => mark_all_read(st).await;
    undo_applied(message_id: String) => undo_applied(st, &message_id).await;
    set_provider_key(provider: String, key: String) => set_provider_key(st, &provider, &key).await;
    delete_provider_key(provider: String) => delete_provider_key(st, &provider).await;
    provider_key_status(provider: String) => provider_key_status(st, &provider).await;
    list_connections() => list_connections(st).await;
    add_connection(provider: String, label: String, key: String) =>
        add_connection(st, &provider, &label, &key).await;
    rename_connection(id: String, label: String) => rename_connection(st, &id, &label).await;
    list_remote_models(refresh: bool, role: Option<String>) =>
        list_remote_models(st, refresh, role.as_deref().unwrap_or("decision")).await;
}
