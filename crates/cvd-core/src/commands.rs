// The IPC command registry: every command the webview can call, declared once.
//
// `for_each_command!(cb)` expands to `cb! { .. }` with the whole table, and each
// transport turns it into what it needs: Tauri wrappers and the handler list
// (desktop), the RPC dispatch (cvd-server), and the ACL list (desktop build.rs,
// which `include!`s this file, so it must stay free of other items).
//
// Shape of the table:
// - `desktop_only`: hand-written in the desktop app; the server refuses them.
// - `per_transport`: hand-written in each transport because the behaviour differs.
// - `st;` then one arm per shared command: `name(arg: Type, ..) -> Ret => body;`
//   where `body` is a `Result<Ret, AppError>` expression that may `.await` and
//   sees `st: &AppState`. Callers bring `services` and the types into scope.
//
// Argument names are snake_case here and camelCase on the wire.

#[macro_export]
macro_rules! for_each_command {
    ($cb:ident) => {
        $cb! {
            desktop_only: [
                get_local_connection,
                save_local_connection,
                test_local_connection,
                save_file_bytes,
                start_apply,
                autofill_bridge,
            ];
            per_transport: [export_cv];
            st;
            get_profile() -> Profile => services::get_profile(st).await;
            save_profile(profile: Profile) -> () => services::save_profile(st, profile).await;
            list_entries(include_archived: bool) -> Vec<Entry> =>
                services::list_entries(st, include_archived).await;
            save_entry(input: EntryInput) -> i64 => services::save_entry(st, input).await;
            delete_entry(id: i64) -> () => services::delete_entry(st, id).await;
            list_skills() -> Vec<Skill> => services::list_skills(st).await;
            create_job(title: String, company: String, raw_text: String, url: String, status_id: Option<i64>) -> i64 =>
                services::create_job(st, &title, &company, &raw_text, &url, status_id).await;
            update_job_tracking(id: i64, status_id: Option<i64>, url: String) -> () =>
                services::update_job_tracking(st, id, status_id, &url).await;
            update_job_description(id: i64, raw_text: String) -> () =>
                services::update_job_description(st, id, &raw_text).await;
            fetch_job_description(url: String) -> String => services::fetch_job_description(&url).await;
            list_job_statuses() -> Vec<JobStatus> => services::list_job_statuses(st).await;
            create_job_status(name: String, color: StatusColor) -> JobStatus =>
                services::create_job_status(st, &name, color).await;
            update_job_status(id: i64, name: String, color: StatusColor) -> JobStatus =>
                services::update_job_status(st, id, &name, color).await;
            delete_job_status(id: i64) -> () => services::delete_job_status(st, id).await;
            reorder_job_statuses(ids: Vec<i64>) -> () => services::reorder_job_statuses(st, ids).await;
            list_job_views() -> Vec<JobView> => services::list_job_views(st).await;
            create_job_view(view: JobViewInput) -> JobView => services::create_job_view(st, &view).await;
            update_job_view(id: i64, view: JobViewInput) -> JobView =>
                services::update_job_view(st, id, &view).await;
            delete_job_view(id: i64) -> () => services::delete_job_view(st, id).await;
            list_jobs() -> Vec<JobDescription> => services::list_jobs(st).await;
            search_jobs(query: String) -> Vec<i64> => services::search_jobs(st, &query).await;
            get_job(id: i64) -> JobDescription => services::get_job(st, id).await;
            start_match(jd_id: i64) -> i64 => services::start_match(st, jd_id).await;
            cancel_match(run_id: i64) -> () => services::cancel_match(st, run_id);
            list_runs(jd_id: i64) -> Vec<MatchRun> => services::list_runs(st, jd_id).await;
            get_scores(run_id: i64) -> Vec<ScoreRow> => services::get_scores(st, run_id).await;
            list_cvs(jd_id: i64) -> Vec<Cv> => services::list_cvs(st, jd_id).await;
            get_ui_setting(key: String) -> Option<String> => services::get_ui_setting(st, &key).await;
            set_ui_setting(key: String, value: String) -> () =>
                services::set_ui_setting(st, &key, &value).await;
            get_apply_defaults() -> ApplyDefaults => services::get_apply_defaults(st).await;
            save_apply_defaults(defaults: ApplyDefaults) -> () =>
                services::save_apply_defaults(st, defaults).await;
            apply_facts() -> Vec<Fact> => services::apply_facts(st).await;
            engine_status() -> EngineStatus => services::engine_status(st).await;
            get_ranked(run_id: i64) -> Ranked => services::get_ranked(st, run_id).await;
            fit_cv(run_id: i64, budget: u8, overrides: Vec<Override>) -> FitOutput =>
                services::fit_cv(st, run_id, budget, overrides).await;
            set_active_model(id: String) -> () => services::set_active_model(st, &id).await;
            set_research_model(id: String) -> () => services::set_research_model(st, &id).await;
            skip_research(run_id: i64, mode: String) -> () => services::skip_research(st, run_id, &mode).await;
            rescore(run_id: i64) -> i64 => services::rescore(st, run_id).await;
            get_company(id: i64) -> Company => services::get_company(st, id).await;
            get_job_company(jd_id: i64) -> Option<Company> => services::get_job_company(st, jd_id).await;
            edit_company(id: i64, profile: CompanyProfile) -> Company =>
                services::edit_company(st, id, profile).await;
            refresh_company(id: i64) -> Company => services::refresh_company(st, id).await;
            get_research_model() -> Option<String> => st.research_model().await;
            get_mail_account() -> Option<MailAccountView> => services::get_mail_account(st).await;
            save_mail_account(host: String, port: u16, address: String, password: Option<String>, folder: Option<String>) -> MailAccountView =>
                services::save_mail_account(st, &host, port, &address, password.as_deref(), folder.as_deref()).await;
            delete_mail_account() -> () => services::delete_mail_account(st).await;
            test_mail_account(host: String, port: u16, address: String, password: Option<String>) -> Vec<String> =>
                services::test_mail_account(st, &host, port, &address, password.as_deref()).await;
            start_scan() -> i64 => services::start_scan(st).await;
            get_scan_status() -> ScanState => Ok(services::get_scan_status(st));
            list_pending_reviews() -> Vec<ProcessedEmailView> => services::list_pending_reviews(st).await;
            list_job_emails(job_id: i64) -> Vec<ProcessedEmailView> => services::list_job_emails(st, job_id).await;
            apply_suggestion(message_id: String) -> ProcessedEmailView =>
                services::apply_suggestion(st, &message_id).await;
            dismiss_suggestion(message_id: String) -> () => services::dismiss_suggestion(st, &message_id).await;
            get_watcher_status() -> WatcherStatus => Ok(services::get_watcher_status(st));
            retry_watcher() -> () => Ok(services::retry_watcher(st));
            list_inbox(filter: InboxFilter) -> Vec<ProcessedEmailView> => services::list_inbox(st, filter).await;
            unread_count() -> i64 => services::unread_count(st).await;
            mark_read(message_id: String) -> () => services::mark_read(st, &message_id).await;
            mark_all_read() -> () => services::mark_all_read(st).await;
            undo_applied(message_id: String) -> ProcessedEmailView => services::undo_applied(st, &message_id).await;
            set_provider_key(provider: String, key: String) -> KeyStatus =>
                services::set_provider_key(st, &provider, &key).await;
            delete_provider_key(provider: String) -> () => services::delete_provider_key(st, &provider).await;
            provider_key_status(provider: String) -> KeyStatus => services::provider_key_status(st, &provider).await;
            list_connections() -> Vec<ConnectionView> => services::list_connections(st).await;
            add_connection(provider: String, label: String, key: String) -> ConnectionView =>
                services::add_connection(st, &provider, &label, &key).await;
            rename_connection(id: String, label: String) -> () => services::rename_connection(st, &id, &label).await;
            list_remote_models(refresh: bool, role: Option<String>) -> Vec<ProviderModels> =>
                services::list_remote_models(st, refresh, role.as_deref().unwrap_or("decision")).await;
        }
    };
}

macro_rules! command_names {
    (
        desktop_only: [$($desktop:ident),* $(,)?];
        per_transport: [$($per:ident),* $(,)?];
        $st:ident;
        $($name:ident ($($arg:ident : $ty:ty),* $(,)?) -> $ret:ty => $body:expr;)*
    ) => {
        /// Shared commands with their snake_case argument names.
        pub const SHARED: &[(&str, &[&str])] = &[$((stringify!($name), &[$(stringify!($arg)),*])),*];
        pub const DESKTOP_ONLY: &[&str] = &[$(stringify!($desktop)),*];
        pub const PER_TRANSPORT: &[&str] = &[$(stringify!($per)),*];
    };
}
for_each_command!(command_names);
