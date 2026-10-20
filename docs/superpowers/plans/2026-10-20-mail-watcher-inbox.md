# Mail Watcher and Inbox Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Classify Gmail in the background over IMAP IDLE while the app is open, and show results in a new Inbox page instead of a popup.

**Architecture:** A long-lived watcher task (`mail/watch.rs`) holds an IDLE connection and calls one shared scan cycle (`mail/cycle.rs`). A cycle runs an optional re-check (`mail/recheck.rs`) and then the existing incremental `run_scan`. The watcher talks to the app through a `WatcherControl` channel stored on `AppState`. The frontend gets an `InboxStore` and an `/inbox` page, and the scan-results popup is removed.

**Tech Stack:** Rust (tokio, async-imap 0.12, sqlx/SQLite), Tauri 2, Angular 22 (signals, Zard UI), Vitest via `ng test`.

**Spec:** `docs/superpowers/specs/2026-10-20-mail-watcher-inbox-design.md`

## Global Constraints

- Migration file: `src-tauri/migrations/0010_inbox.sql` (`0009` belongs to PR #48).
- Re-IDLE interval: 10 minutes. Backoff: 5 s doubling to a 5 min cap, reset after a successful connect.
- Re-check window: rows processed in the last 30 days (`processed_at >= datetime('now','-30 days')`). Threshold `JOB_RELATED_THRESHOLD` (0.5).
- Stored body: `EmailInput.body` (already truncated to `BODY_CHARS` = 6000), only when `job_related >= 0.5`.
- Setting key for the re-check watermark: `mail_recheck_job_id`.
- Events: `mail://watcher`, `inbox://changed`. Existing `scan://progress` and `scan://finished` stay. `scan://finished` gains `source`.
- Fatal kinds (pause the watcher): `MailAuth`, `ProviderAuth`, `ProviderCredits`. Everything else that fails a cycle is transient (backoff).
- Undo tooltip copy: "The status was changed after this update."
- Inbox ordering: `pending_review` first, then `processed_at DESC`. `received_at` holds the raw RFC 2822 `Date` header and does not sort as text, so `processed_at` is used. A re-matched email surfacing at the top is intended. (Deviation from the spec's "received_at"; update the spec line in Task 1.)
- Rust checks: `cd src-tauri && cargo test` and `cargo clippy --all-targets -- -D warnings`. Frontend: `bun run test`.

## Review Focus

1. **Laptop sleep / half-open TCP:** IDLE can hang forever on a dead socket. The 10-min wall-clock re-IDLE has to use a `tokio::time` deadline, not async-imap's keepalive-reset timeout, so a dead connection fails and reconnects. Test in Task 5: `idle_that_never_returns_is_reissued_after_ten_minutes`.
2. **Manual "Check now" while the watcher is scanning:** this must not double-process emails or error noisily. The slot is shared. Manual start returns "A scan is already running." and the UI keeps showing the running scan. Test in Task 6: `start_scan_while_slot_held_errors`.
3. **New mail arriving mid-scan:** it must be picked up without a second concurrent scan. Test in Task 5: `signals_during_scan_coalesce_into_one_follow_up`.
4. **Upgrade with existing history:** no unread flood and no mass re-classification. Tests in Task 1 (`migration_marks_existing_rows_read`) and Task 3 (`first_run_initialises_watermark_without_rechecking`).
5. **Undo after a manual status change:** this must never overwrite the user's choice. Test in Task 1: `undo_blocked_after_manual_status_change`.

---

### Task 1: Migration and inbox store

**Files:**
- Create: `src-tauri/migrations/0010_inbox.sql`
- Modify: `src-tauri/src/store/email.rs`, `src-tauri/src/store/mod.rs` (re-exports)
- Test: `src-tauri/src/store/tests.rs`
- Modify: spec "Ordering" line → `processed_at`

**Interfaces:**
- Produces:
  - `ScanSource { Watch, Recheck, Manual }` (serde `snake_case`). `Store::create_scan_run(source: ScanSource) -> Result<i64>`. Every existing caller is updated.
  - `NewProcessed` gains `job_related: Option<f64>` and `body: Option<String>`.
  - `ProcessedEmail` gains `job_related: Option<f64>`, `body: Option<String>`, `read_at: Option<String>`, `undone_at: Option<String>`.
  - `Store::record_processed(&NewProcessed, apply_status: Option<i64>)` becomes an upsert. On conflict it overwrites every classified field plus `run_id` and `processed_at = CURRENT_TIMESTAMP`, and sets `read_at = CASE WHEN excluded.outcome = 'ignored' THEN processed_email.read_at ELSE NULL END`.
  - `InboxFilter { All, Review, Unread }` (serde `snake_case`). `Store::list_inbox(filter) -> Result<Vec<ProcessedEmail>>` excludes `ignored`; `Review` = `pending_review` only; `Unread` = `read_at IS NULL`. Ordered `outcome = 'pending_review' DESC, processed_at DESC, message_id`.
  - `Store::unread_count() -> Result<i64>`: non-ignored rows with `read_at IS NULL`.
  - `Store::mark_read(message_id) -> Result<()>`, `Store::mark_all_read() -> Result<()>`.
  - `Store::undo_applied(message_id) -> Result<ProcessedEmail>`. Errors with `StoreError::Invalid("The status was changed after this update.")` unless the row is `applied`, not undone, and the job's `status_id == suggested_status_id`. Otherwise it sets the job to `previous_status_id` and sets `undone_at`, in one transaction.
  - `apply_suggestion` and `dismiss_suggestion` also set `read_at = CURRENT_TIMESTAMP`.
  - `Store::recheck_candidates() -> Result<Vec<ProcessedEmail>>`: `outcome = 'ignored' AND processed_at >= datetime('now','-30 days') AND (job_related >= 0.5 OR job_related IS NULL)`.
  - `Store::max_job_id() -> Result<i64>` (0 when there are no jobs).

- [ ] **Step 1: Write the migration.** Add the columns `job_related REAL`, `body TEXT`, `read_at TEXT`, and `undone_at TEXT` to `processed_email`. Then `UPDATE processed_email SET read_at = processed_at;`. Add `scan_run.source TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('watch','recheck','manual'))` and `CREATE INDEX processed_email_read_idx ON processed_email(read_at);`.
- [ ] **Step 2: Write the failing store tests** in `store/tests.rs`, using `Store::open("sqlite::memory:")` and the `NewProcessed` literal style from `mail/scan_tests.rs`:
  - `migration_marks_existing_rows_read`: insert a row, `UPDATE processed_email SET read_at = NULL`, execute `include_str!("../../migrations/0010_inbox.sql")`'s backfill statement (`UPDATE processed_email SET read_at = processed_at`), and assert `read_at == Some(processed_at)`.
  - `new_rows_are_unread_and_counted`: one `applied` and one `ignored` row give `unread_count() == 1`.
  - `upsert_rematch_marks_unread`: insert `ignored`, `mark_all_read`, then upsert the same `message_id` as `pending_review`. Assert `read_at == None` and that the outcome is updated.
  - `upsert_still_ignored_keeps_read`: same as above, but the upsert stays `ignored`. Assert `read_at` is kept.
  - `list_inbox_orders_review_first_and_hides_ignored`.
  - `list_inbox_unread_filter`.
  - `undo_restores_previous_status`: job at status A, row applied A→B with the job at B. After undo the job is at A and `undone_at` is `Some`.
  - `undo_blocked_after_manual_status_change`: the job was moved to C by hand. Undo returns `Err` and the job stays at C.
  - `undo_twice_errors`.
  - `apply_and_dismiss_mark_read`.
  - `recheck_candidates_selects_job_related_and_legacy`: covers rows with scores 0.7 and NULL (selected), 0.2 (not selected), an `applied` row (not selected), and an ignored 0.7 row with `processed_at` 40 days ago, set via SQL (not selected).
- [ ] **Step 3: Run** `cd src-tauri && cargo test store::` and confirm compile failures or FAIL.
- [ ] **Step 4: Implement** the interfaces above. Fix every `create_scan_run()` and `NewProcessed` construction site; `scan::begin_run` gets a `source` parameter in Task 2, so pass `ScanSource::Manual` for now.
- [ ] **Step 5: Run** `cargo test`. Expect all PASS.
- [ ] **Step 6: Commit** with `feat(mail): inbox columns, upsert, undo and read state`.

### Task 2: Scan pipeline records score and body, reusable per-email step

**Files:**
- Modify: `src-tauri/src/mail/scan.rs`, `src-tauri/src/mail/imap.rs`
- Test: `src-tauri/src/mail/scan_tests.rs`

**Interfaces:**
- Consumes: Task 1 `NewProcessed` fields and `ScanSource`.
- Produces:
  - `begin_run(store, source: ScanSource) -> Result<i64, AppError>`.
  - `pub const INBOX_CHANGED_EVENT: &str = "inbox://changed";`, emitted with `{ "message_id": … }` after every recorded row whose outcome is not `ignored`.
  - `finished_payload` includes `"source"`. `run_scan(deps, run_id, source)`.
  - `pub struct EmailMeta { message_id, subject, sender, received_at }`, built `From<&MailHeader>`.
  - `pub(crate) async fn process_email(deps: &ScanDeps<'_>, meta: &EmailMeta, body: &str, run_id: i64) -> Result<Tally, AppError>`: the current `handle_header` body after the fetch. `handle_header` becomes a fetch followed by `process_email`. `Tally` becomes `pub(crate)`.
  - `MailSource::body_by_message_id(&self, message_id: &str) -> Result<Option<String>, MailError>`. The IMAP implementation uses `UID SEARCH HEADER Message-ID "<id>"`, then `body_text(uid)`; no hit gives `Ok(None)`.

- [ ] **Step 1: Write the failing tests** in `scan_tests.rs`. Extend `FakeMail` with `body_by_message_id`, looking up the header whose `message_id` matches.
  - `ignored_email_stores_score_without_body`: with `ignoring()` (job_related 0.1), the row has `job_related == Some(0.1)` and `body == None`.
  - `job_related_unmatched_stores_body`: `scripted(0.9, "no_match", …)` gives outcome `Ignored`, `job_related == Some(0.9)`, and `body == Some("body of 1")`.
  - `matched_email_stores_body_and_emits_inbox_changed`: capture emitted events with a `Mutex<Vec<String>>` emitter and assert one `inbox://changed`.
  - `finished_payload_carries_source`: run with `ScanSource::Watch` and assert the `scan://finished` payload has `source == "watch"`.
- [ ] **Step 2: Run** `cargo test mail::scan` and confirm FAIL.
- [ ] **Step 3: Implement.** In `process_email`, set `row.job_related = Some(classified.job_related)` and `row.body = (classified.job_related >= JOB_RELATED_THRESHOLD).then(|| email.body.clone())` on every branch, including `Decision::Ignored`.
- [ ] **Step 4: Run** `cargo test`. Expect PASS.
- [ ] **Step 5: Commit** with `feat(mail): store job_related score and body; emit inbox://changed`.

### Task 3: Re-check on new job

**Files:**
- Create: `src-tauri/src/mail/recheck.rs` (register it in `mail/mod.rs`)
- Test: `src-tauri/src/mail/recheck_tests.rs`, using `#[path]` the same way `scan_tests.rs` does

**Interfaces:**
- Consumes: `process_email`, `EmailMeta`, `Tally`, `MailSource::body_by_message_id`, `Store::recheck_candidates`, `Store::max_job_id`, and `Store::get_setting`/`set_setting`.
- Produces:
  - `pub const RECHECK_KEY: &str = "mail_recheck_job_id";`
  - `pub async fn needs_recheck(store: &Store) -> Result<bool, AppError>`. When the key is absent it writes the current `max_job_id` and returns `false`. Otherwise it returns `max_job_id > stored`.
  - `pub async fn run_recheck(deps: ScanDeps<'_>, run_id: i64) -> Result<ScanSummary, AppError>`. It snapshots `max_job_id` at the start. For each candidate it uses `row.body`, or `body_by_message_id` when that is `None`; `Ok(None)` skips the row. It then calls `process_email`. It finishes the run like `run_scan` does (`scan://finished` with source `recheck`). The setting is updated to the snapshot only on success.

- [ ] **Step 1: Write the failing tests**, reusing the `scan_tests` helpers (move `FakeMail`, `seeded`, `scripted`, `chat_endpoint` into a shared `mail/test_support.rs` behind `#[cfg(test)]`):
  - `first_run_initialises_watermark_without_rechecking`: `needs_recheck` returns `false`, and the setting equals the current max id.
  - `new_job_triggers_recheck`: after initialising, create a job, and `needs_recheck` returns `true`.
  - `recheck_rematches_stored_body_without_imap`: an ignored row with score 0.9 and a body; the engine now matches the new job. The row becomes `applied` or `pending_review`, `read_at` is `None`, and `FakeMail.fetched()` is empty.
  - `recheck_refetches_legacy_row`: a row with `job_related` NULL and no body, whose header is present in `FakeMail`. The body is fetched by message id and the row is updated with `job_related` set.
  - `recheck_skips_missing_legacy_message`: the message is absent from `FakeMail`. The row is unchanged and the run succeeds.
  - `failed_recheck_keeps_watermark`: an engine returning `EngineError::Unauthorized` makes `run_recheck` return `Err`, and the setting is unchanged.
- [ ] **Step 2: Run** `cargo test mail::recheck` and confirm FAIL.
- [ ] **Step 3: Implement** `recheck.rs`.
- [ ] **Step 4: Run** `cargo test`. Expect PASS.
- [ ] **Step 5: Commit** with `feat(mail): re-check ignored job-related emails when a job is added`.

### Task 4: Shared scan cycle

**Files:**
- Create: `src-tauri/src/mail/cycle.rs`
- Modify: `src-tauri/src/state.rs` (slot-freed `Notify`), `src-tauri/src/commands.rs` (`start_scan` uses the cycle)
- Test: `src-tauri/src/commands_tests.rs`

**Interfaces:**
- Produces:
  - `AppState::scan_freed(&self) -> Arc<tokio::sync::Notify>`. `ScanGuard::drop` calls `notify_waiters()`.
  - `pub enum CycleError { Busy, NoAccount, Failed(AppError) }`.
  - `pub struct Prepared { guard: ScanGuard, deps owned: account, folder, engine, chat, store, emit }` (owned so it can move into a task).
  - `pub async fn prepare(st: &AppState) -> Result<Prepared, CycleError>`. It claims the slot (`Busy`), loads the account (`NoAccount`), then the model, engine and chat endpoint (`Failed`). It also refuses with the existing `begin_run` precondition ("Add a job and a status first."), checked here *before* creating any run row.
  - `pub async fn run_cycle(p: Prepared, source: ScanSource, run_id: i64) -> Result<ScanSummary, AppError>`. The caller creates `run_id` with `scan::begin_run(&store, source)` after `prepare`. It builds `ImapSource` and `ScanDeps`. If `recheck::needs_recheck`, it runs `run_recheck` first (creating its own `Recheck` run), then `run_scan(deps, run_id, source)`. The guard drops at the end.
  - `start_scan` keeps `Res<i64>`: `prepare` (mapping `Busy` → `AppError::Io("A scan is already running.")`, `NoAccount` → the existing "Connect Gmail…" error, and `Failed(e)` → `e`), then `begin_run(Manual)`, then spawn `run_cycle`, returning `run_id`.
- [ ] **Step 1: Write the failing tests** in `commands_tests.rs`, using the existing `AppState` test constructor with `EngineSource::Fixed`:
  - `start_scan_while_slot_held_errors`: hold `st.begin_scan()`, then `start_scan` errors with "A scan is already running."
  - `prepare_without_account_is_no_account`.
  - `scan_freed_notifies_on_guard_drop`: `tokio::spawn` a waiter on `notified()`, drop the guard, and the waiter completes within 1 s.
- [ ] **Step 2: Run** `cargo test commands` and confirm FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test`. Expect PASS.
- [ ] **Step 5: Commit** with `refactor(mail): shared scan cycle with re-check`.

### Task 5: Watcher state machine

**Files:**
- Create: `src-tauri/src/mail/watch.rs`, `src-tauri/src/mail/watch_tests.rs`
- Modify: `src-tauri/src/mail/imap.rs` (`ImapIdle`), `src-tauri/Cargo.toml` (dev-dependency `tokio = { version = "1", features = ["test-util", "macros", "rt"] }`)

**Interfaces:**
- Produces:
  - `pub const WATCHER_EVENT: &str = "mail://watcher";`
  - `pub enum WatchState { Connecting, Watching, Scanning, Paused, Stopped }` (serde lowercase).
  - `#[derive(Serialize, Clone)] pub struct WatcherStatus { state: WatchState, error: Option<String>, error_kind: Option<String>, last_checked_at: Option<String> }`. `last_checked_at` is RFC 3339, set after each successful cycle.
  - `pub enum Control { Restart, Stop, Nudge, Retry }`.
  - `pub struct WatcherControl`. It is created in `AppState::new`, with `send(&self, Control)` (ignoring a closed channel), `status(&self) -> WatcherStatus`, and `take_receiver(&self) -> Option<UnboundedReceiver<Control>>`.
  - `pub enum IdleEvent { NewData, Timeout }`.
  - `#[async_trait] pub trait IdleConn: Send { async fn wait(&mut self) -> Result<IdleEvent, MailError>; }`. The contract: return `Timeout` after at most `REIDLE` of wall-clock time.
  - `#[async_trait] pub trait WatchDeps: Send + Sync { async fn connect(&self) -> Result<Option<Box<dyn IdleConn>>, MailError>; async fn cycle(&self) -> Result<(), CycleError>; fn scan_freed(&self) -> Arc<Notify>; }`. `connect` returns `Ok(None)` when no account is saved.
  - `pub const REIDLE: Duration = 10 min`, `BACKOFF_MIN = 5 s`, `BACKOFF_MAX = 300 s`.
  - `pub async fn run_watcher(deps: Arc<dyn WatchDeps>, rx: UnboundedReceiver<Control>, status: Arc<Mutex<WatcherStatus>>, emit: Emitter)`. It emits `WATCHER_EVENT` with the status on every state change.
  - `ImapIdle` (production `IdleConn`): owns `Option<Session>`, and `EXAMINE`s the folder on connect. `wait()` runs `idle()`, `init()`, then `wait_with_timeout(REIDLE)`, raced against `tokio::time::sleep(REIDLE)`. When the sleep wins it drops the `StopSource` and awaits the IDLE future. It then calls `done()` and puts the session back, mapping `NewData`→`NewData` and the rest→`Timeout`.
- **Loop semantics** (the algorithm the tests pin):
  1. `Connecting` → `connect()`. `Ok(None)` → `Stopped`; wait for `Restart`. `Err(Auth)` → `Paused`. Any other `Err` → backoff sleep (doubling, capped), then retry.
  2. On connect: reset the backoff and run a cycle (`Scanning`). The cycle results: `Ok` → `Watching`, with `last_checked_at` set. `Busy` → wait on `scan_freed`, then run the cycle again. `NoAccount` → `Stopped`. `Failed(e)` with a fatal kind → `Paused`; otherwise drop the connection and back off.
  3. `Watching` → `select!` over `conn.wait()` and `rx.recv()`. `NewData` or `Nudge` → run a cycle (a `Nudge` drops and reconnects the connection afterwards, since its IDLE future was cancelled). `Timeout` → wait again. `Err` → back off and reconnect.
  4. While a cycle runs, `rx` is drained in a `select!` alongside the cycle future. Any number of `Nudge`s set one `dirty` flag; when the cycle ends, `dirty` triggers exactly one more cycle. `Stop` or `Restart` received mid-cycle is applied once the cycle finishes (cycles are not cancelled mid-email). Mail arriving mid-cycle needs no flag: the server queues the `EXISTS` and the next `IDLE` reports it.
  5. `Paused`: wait only for `Retry` or `Restart` → `Connecting`. `Stop` from any state → `Stopped`, dropping the connection. `Restart` from any state → `Connecting`, with the backoff reset.
- [ ] **Step 1: Write the failing tests** with `#[tokio::test(start_paused = true)]`. Use a `FakeDeps` that has a scripted queue of `connect` results and `IdleEvent`s (an `IdleConn` whose `wait` pops from a shared `VecDeque`, with `std::future::pending()` once it is empty), a cycle counter, an optional cycle delay (`tokio::time::sleep`), and scripted cycle results. Assert on cycle counts and on collected `mail://watcher` states.
  - `connect_runs_catch_up_cycle`: one cycle after connect, then the state is `watching`.
  - `new_data_runs_cycle`.
  - `signals_during_scan_coalesce_into_one_follow_up`: the cycle takes 30 s; send three `Nudge`s during it. The total is exactly 2 cycles.
  - `connection_error_backs_off`: the first two connects fail with `Connection`. The cycle runs only after `5 s + 10 s` of advanced time (`tokio::time::advance`), not before.
  - `backoff_caps_at_five_minutes`.
  - `auth_failure_pauses_until_retry`: `connect` → `Err(Auth)`, then the state is `paused` with `error_kind == "MailAuth"`. No reconnect happens after 1 h of advanced time. `Retry` leads to connect and a cycle.
  - `fatal_cycle_error_pauses`: `Failed(AppError::ProviderCredits)` → `paused`.
  - `transient_cycle_error_backs_off`: `Failed(AppError::ProviderUnavailable)` → backoff, then reconnect.
  - `busy_waits_for_slot_then_scans`: the first cycle returns `Busy`; after `scan_freed().notify_waiters()`, a second cycle runs.
  - `no_account_stops_and_restart_resumes`.
  - `idle_that_never_returns_is_reissued_after_ten_minutes`: this tests `ImapIdle`'s contract through a wrapper. Pin it with a unit test on a helper `fn reidle_deadline<F: Future>(fut: F) -> impl Future<Output = Option<F::Output>>` that `ImapIdle::wait` uses. With `pending()`, it resolves to `None` after `REIDLE`.
- [ ] **Step 2: Run** `cargo test mail::watch` and confirm FAIL.
- [ ] **Step 3: Implement** `watch.rs` and `ImapIdle`.
- [ ] **Step 4: Run** `cargo test` and `cargo clippy --all-targets -- -D warnings`. Expect PASS.
- [ ] **Step 5: Commit** with `feat(mail): IMAP IDLE watcher with backoff, pause and coalescing`.

### Task 6: Wire the watcher and inbox commands into the app

**Files:**
- Modify: `src-tauri/src/state.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`
- Test: `src-tauri/src/commands_tests.rs`

**Interfaces:**
- Consumes: Tasks 1–5.
- Produces:
  - `AppState.watcher: WatcherControl`.
  - `AppWatchDeps { handle: tauri::AppHandle }` implements `WatchDeps` through `handle.state::<AppState>()`: `connect` loads the account and folder and builds `ImapIdle`; `cycle` calls `prepare`, then `run_cycle(…, Watch)`.
  - In `lib.rs` setup, after `app.manage(state)`, take the receiver and `tauri::async_runtime::spawn(run_watcher(…))`, then `send(Control::Restart)` to start it.
  - `save_mail_account` → `send(Restart)`. `delete_mail_account` → `send(Stop)`. `create_job` → `send(Nudge)`. The AI key/model save commands that already exist (those calling `reload_providers` or `set_active_model`) → `send(Retry)`, but only if the state is `Paused`.
  - New commands (fn + `ipc` wrapper + `generate_handler!` entry):
    - `get_watcher_status() -> WatcherStatus`
    - `retry_watcher() -> ()`
    - `list_inbox(filter: InboxFilter) -> Vec<ProcessedEmailView>`
    - `unread_count() -> i64`
    - `mark_read(message_id) -> ()`
    - `mark_all_read() -> ()`
    - `undo_applied(message_id) -> ProcessedEmailView`
  - Each mutating inbox command emits `inbox://changed`. `undo_applied` also emits `JOB_UPDATED_EVENT`, like `apply_suggestion` should.
  - `list_scan_results` is removed (its only user was the popup). `list_pending_reviews` is kept for the Jobs badge.
- [ ] **Step 1: Write the failing tests** in `commands_tests.rs`:
  - `save_mail_account_restarts_watcher`: take the receiver from `st.watcher` before calling, then assert it receives `Control::Restart`.
  - `delete_mail_account_stops_watcher`.
  - `create_job_nudges_watcher`.
  - `undo_applied_emits_job_updated`: capture events with the test emitter.
  - `list_inbox_returns_views_with_gmail_url`.
- [ ] **Step 2: Run** `cargo test commands` and confirm FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test` and `cargo clippy --all-targets -- -D warnings`. Expect PASS.
- [ ] **Step 5: Commit** with `feat(mail): run the watcher in the app and expose inbox commands`.

### Task 7: Frontend IPC and InboxStore

**Files:**
- Modify: `src/app/core/ipc/types.ts`, `commands.ts`, `events.ts`, `src/app/core/stores/scan.store.ts`
- Create: `src/app/core/stores/inbox.store.ts`, `src/app/core/stores/inbox.store.spec.ts`
- Modify: `src/app/core/stores/scan.store.spec.ts`

**Interfaces:**
- Produces:
  - Types:
    - `ProcessedEmail` gains `job_related: number | null`, `body: string | null`, `read_at: string | null`, and `undone_at: string | null`.
    - `ScanFinished` gains `source: 'watch' | 'recheck' | 'manual'`.
    - `WatcherStatus { state: 'connecting' | 'watching' | 'scanning' | 'paused' | 'stopped'; error: string | null; error_kind: AppErrorKind | null; last_checked_at: string | null }`.
    - `InboxFilter = 'all' | 'review' | 'unread'`.
  - Commands: `getWatcherStatus`, `retryWatcher`, `listInbox(filter)`, `unreadCount`, `markRead(messageId)`, `markAllRead`, `undoApplied(messageId)`. `listScanResults` is removed.
  - Events: `onWatcherStatus(cb)`, `onInboxChanged(cb)`.
  - `InboxStore` (root): signals `items`, `filter`, `unread`, `watcher`; `ensureReady()`, subscribed once to `mail://watcher`, `inbox://changed`, and `scan://finished`, each of which refreshes `items` and `unread`; `setFilter(f)`, `markRead(id)`, `markAllRead()`, `apply(id)`, `dismiss(id)`, `undo(id)`, `retry()`.
  - `ScanStore` drops `finishedRunId`, `acknowledgeFinished`, and `finishedSummary`. It gains `manualStarted` (set by `start()`). On `scan://finished` with `source === 'manual'` and `manualStarted`, it shows a toast via `ZardSonnerService`: `${applied + pending} updates`, `"No new updates"` when both are 0, or the error message on failure. It then clears the flag.
- [ ] **Step 1: Write the failing specs** with `mockTauri` from `src/testing/tauri-mock`:
  - `inbox.store.spec.ts`: `loads items and unread count on ensureReady`; `refreshes on inbox://changed`; `setFilter re-queries with the filter`; `undo calls undo_applied and refreshes`; `tracks watcher status events`.
  - `scan.store.spec.ts`: `toasts once for a manual scan`; `does not toast for watch scans`. Remove the `finishedRunId` tests.
- [ ] **Step 2: Run** `bun run test` and confirm FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `bun run test`. Expect PASS for these specs; later tasks fix the components that still reference removed members.
- [ ] **Step 5: Commit** with `feat(inbox): IPC bindings and InboxStore`.

### Task 8: Inbox page, route, sidebar badge; remove the popup

**Files:**
- Create: `src/app/features/inbox/inbox-page.component.ts`, `src/app/features/inbox/inbox-row.component.ts`, `src/app/features/inbox/inbox.spec.ts`
- Modify: `src/app/app.routes.ts` (`{ path: 'inbox', loadComponent: … }`), `src/app/app.routes.spec.ts`, `src/app/shell/app-sidebar.component.ts`, `src/app/shell/shell.spec.ts`, `src/app/features/jobs/jobs-page.component.ts`, `src/app/features/jobs/jobs.spec.ts`
- Delete: `src/app/features/jobs/scan-results-dialog.component.ts`, `scan-results-dialog.spec.ts`

**Interfaces:**
- Consumes: `InboxStore`, `ScanStore`, `JobsStore`, `JobStatusesStore`, `StatusPillComponent`, `openJobUrl`, and `messageFor`.
- Behaviour (copy is fixed by the spec):
  - **Header status line:**
    - `watching`: "Watching Gmail · checked {relative}"
    - `scanning`: "Checking {checked} / {total} emails…" from `ScanStore.progress()`, otherwise "Checking emails…"
    - `connecting`: "Connecting…"
    - `paused`: "Paused: {error}", plus a link to `/settings` for `MailAuth`/`MailUnavailable` or to `/ai` for provider kinds, plus a **Retry** button
    - `stopped` or no account: "Connect Gmail in Settings › Integrations."
  - **Header buttons:** **Check now** (`ScanStore.start()`, disabled while `running()` or when there is no account) and **Mark all read**.
  - **Tabs:** All · Needs review · Unread, synced to the `?filter=` query param.
  - **Row variants:** Applied (old → new pills and **Undo**), Needs review (suggested pill, confidence as `90%`, **Apply** and **Dismiss**), No change (current pill and "No change"), Undone (struck-through pills and "Undone"), Dismissed (muted, "Dismissed").
  - **Undo** is disabled unless `job.status_id === row.suggested_status_id && !row.undone_at`; when disabled it carries the tooltip "The status was changed after this update."
  - **Row content:** the unread dot shows while `read_at === null`. Expanding a row shows `body` in a `whitespace-pre-wrap` scroll box (`max-h-64`) and calls `markRead`.
  - **Sidebar:** an "Inbox" link before "Jobs" with the `lucideInbox` icon. It shows a `tabular-nums` badge with `InboxStore.unread()` when > 0, or a red dot (`aria-label="Mail watcher paused"`) when `watcher().state === 'paused'`.
  - **Jobs page:** the scan button and the dialog are removed. The `N to review` badge becomes `<a routerLink="/inbox" [queryParams]="{ filter: 'review' }">`.
- [ ] **Step 1: Write the failing specs:**
  - `inbox.spec.ts`: `renders each row variant`; `pins review rows first` (the order comes from the backend; assert that the DOM order matches the mocked order); `Apply calls apply_suggestion`; `Undo disabled after manual status change shows tooltip`; `expanding a row shows the body and marks it read`; `tabs update the filter query param`; `paused watcher shows error, settings link and Retry`; `Check now disabled while running`.
  - `shell.spec.ts`: `sidebar shows unread badge`; `sidebar shows paused dot`.
  - `jobs.spec.ts`: replace the scan-button and popup tests with `review badge links to inbox review filter`.
  - `app.routes.spec.ts`: `/inbox` resolves.
- [ ] **Step 2: Run** `bun run test` and confirm FAIL.
- [ ] **Step 3: Implement** the components, following `scan-results-dialog.component.ts`'s row markup before deleting it, and delete the dialog files.
- [ ] **Step 4: Run** `bun run test` and `bun run build`. Expect PASS with no type errors.
- [ ] **Step 5: Commit** with `feat(inbox): Inbox page and sidebar badge; remove scan popup`.

### Task 9: End-to-end check in the running app

- [ ] **Step 1:** Run `bun run tauri dev` with a connected Gmail account. Confirm the sidebar shows Inbox and the status reads "Watching Gmail".
- [ ] **Step 2:** Send yourself a job-like email. Within a minute, confirm it appears in the Inbox unread, with no popup.
- [ ] **Step 3:** Change the app password to a wrong one in Settings. Confirm the watcher shows "Paused" and the sidebar shows the red dot. Restore it and confirm the watcher resumes.
- [ ] **Step 4:** Add a job for a company with an earlier, ignored but job-related email. Confirm that email appears in the Inbox.
- [ ] **Step 5:** Fix anything found, re-run `cargo test` and `bun run test`, and commit.
