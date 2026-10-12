# Gmail Job Updates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A "Read emails for updates" button scans the last 30 days of Gmail over IMAP, classifies job-related mail with System One, auto-applies confident status changes, and shows the results in a popup.

**Architecture:** A Rust `mail/` module (IMAP source, pure decision logic, System One question builder, chat summary call, scan orchestration) behind a `MailSource` trait so the pipeline is tested with fakes. Results persist in `processed_email` and `scan_run` tables (migration 0006). The Angular side adds an Integrations card to Settings, a shared scan store, the Jobs page button and badge, and the results dialog.

**Tech Stack:** Rust (Tauri 2, sqlx/SQLite, reqwest, wiremock for tests), IMAP via `async-imap` with rustls, `mailparse` for RFC 5322/2047 parsing, `html2text` (already a dependency). Angular with Zard UI, signals, Vitest-style specs.

**Spec:** `docs/superpowers/specs/2026-10-12-gmail-job-updates-design.md`

## Global Constraints

- Auto-apply requires `status_changed` probability ≥ 0.85 AND `new_status` confidence ≥ 0.85.
- Job-related requires `job_related` probability ≥ 0.5.
- Scan window: `SINCE` today minus 30 days, folder `[Gmail]/All Mail`, all read states.
- IMAP defaults: host `imap.gmail.com`, port `993`, TLS.
- Settings keys: `mail.host`, `mail.port`, `mail.address`. Secret provider key: `gmail`.
- System One model: `ACTIVE_MODEL_KEY`. Chat model for summaries: `RESEARCH_MODEL_KEY` (the "Company research" role).
- Outcome strings (serde `snake_case`): `ignored`, `no_change`, `applied`, `pending_review`, `dismissed`.
- Events: `scan://progress` with `{ checked, total }`; `scan://finished` with `{ run_id, status, error, checked, job_related, applied, pending, no_change, failed }` (the `ScanSummary` fields).
- Migration file: `src-tauri/migrations/0006_email_scan.sql`.
- Commit trailer on every commit: `Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>`.

**Changes from the spec (confirm before implementing):**
1. Processed key is the `Message-ID` header, not `X-GM-MSGID`. The IMAP library does not expose `X-GM-MSGID` cleanly, and `Message-ID` is unique per message. Column: `message_id`.
2. `processed_email` also stores `previous_status_id`, so the popup can show old → new for applied rows.
3. Choice confidence = `probabilities[chosen]`. `Answer::Choice` has no separate confidence field.
4. A scan refuses to start with zero jobs or zero statuses, with an error message.
5. Summary input body is truncated to 6000 characters (on a char boundary).
6. Jobs and statuses are re-read from the DB before each email.

## Review Focus

1. **HTML-only email** (no text/plain part): classification and summary get readable text, not an empty body. Pinned in Task 3.
2. **Very large body** (newsletter, multi-MB): truncated to 6000 chars on a char boundary, with no panic on multibyte text. Pinned in Task 5.
3. **RFC 2047 encoded subject or sender** (`=?UTF-8?Q?...?=`): decoded before display and before the model sees it. Pinned in Task 3.
4. **Job with no company or URL, or a job with no status**: the choice option still uses the stable id, and the label falls back to the existing `jobLabel` behavior. Pinned in Task 5 and Task 12.
5. **Connection drop mid-scan, or thousands of messages in 30 days**: headers fetched in chunks of 200, and a fatal error keeps rows already recorded. Pinned in Task 3 and Task 7.

---

### Task 1: Migration and processed-email store

**Files:**
- Create: `src-tauri/migrations/0006_email_scan.sql`
- Create: `src-tauri/src/store/email.rs`
- Modify: `src-tauri/src/store/mod.rs` (add `mod email;` beside `mod companies;`, and `pub use email::{Outcome, ProcessedEmail, NewProcessed, ScanStatus, ScanRun};`)
- Test: `src-tauri/src/store/tests.rs`

**Interfaces:**
- Produces:
  - `enum Outcome { Ignored, NoChange, Applied, PendingReview, Dismissed }`
  - `struct NewProcessed { message_id: String, run_id: i64, received_at: String, subject: String, sender: String, matched_job_id: Option<i64>, previous_status_id: Option<i64>, suggested_status_id: Option<i64>, confidence: Option<f64>, summary: Option<String>, outcome: Outcome }`
  - `struct ProcessedEmail` = `NewProcessed` fields plus `processed_at: String`
  - `enum ScanStatus { Running, Done, Failed }`
  - `struct ScanRun { id: i64, started_at: String, finished_at: Option<String>, status: ScanStatus, error: Option<String>, checked: i64, failed: i64 }`
  - `Store::create_scan_run(&self) -> Result<i64>`
  - `Store::finish_scan_run(&self, run_id: i64, status: ScanStatus, error: Option<&str>, checked: i64, failed: i64) -> Result<()>`
  - `Store::get_scan_run(&self, run_id: i64) -> Result<Option<ScanRun>>`
  - `Store::processed_ids(&self) -> Result<HashSet<String>>`
  - `Store::record_processed(&self, p: &NewProcessed, apply_status: Option<i64>) -> Result<()>`: one transaction; when `apply_status` is `Some` and `matched_job_id` is `Some`, also sets that job's `status_id`.
  - `Store::get_processed(&self, message_id: &str) -> Result<Option<ProcessedEmail>>`
  - `Store::list_run_results(&self, run_id: i64) -> Result<Vec<ProcessedEmail>>`
  - `Store::list_pending_reviews(&self) -> Result<Vec<ProcessedEmail>>`
  - `Store::apply_suggestion(&self, message_id: &str) -> Result<ProcessedEmail>`: one transaction; sets the job's `status_id` to `suggested_status_id` and the outcome to `applied`. Errors `NotFound` when the row, the job, or the suggested status is missing.
  - `Store::dismiss_suggestion(&self, message_id: &str) -> Result<()>`: sets outcome `dismissed`.

- [ ] **Step 1: Write the migration**

Tables, exactly:
- `scan_run(id INTEGER PRIMARY KEY, started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, finished_at TEXT, status TEXT NOT NULL CHECK (status IN ('running','done','failed')), error TEXT, checked INTEGER NOT NULL DEFAULT 0, failed INTEGER NOT NULL DEFAULT 0)`
- `processed_email(message_id TEXT PRIMARY KEY, run_id INTEGER NOT NULL REFERENCES scan_run(id) ON DELETE CASCADE, processed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, received_at TEXT NOT NULL, subject TEXT NOT NULL, sender TEXT NOT NULL, matched_job_id INTEGER REFERENCES job_description(id) ON DELETE SET NULL, previous_status_id INTEGER REFERENCES job_status(id) ON DELETE SET NULL, suggested_status_id INTEGER REFERENCES job_status(id) ON DELETE SET NULL, confidence REAL, summary TEXT, outcome TEXT NOT NULL CHECK (outcome IN ('ignored','no_change','applied','pending_review','dismissed')))`
- Index: `processed_email(outcome)`.

- [ ] **Step 2: Write failing tests** in `store/tests.rs`:
  - `record_then_list_run_results_round_trips_fields`: record one `Applied` row, then assert `list_run_results(run)` returns it with the same subject, summary, and `outcome == Outcome::Applied`.
  - `processed_ids_contains_every_recorded_message`: record two rows, assert the set contains both ids.
  - `record_with_apply_status_updates_the_job`: create a job with status A, record a row with `apply_status = Some(B)`, assert `get_jd(job).status_id == Some(B)`.
  - `apply_suggestion_sets_status_and_outcome`: record a `PendingReview` row, call `apply_suggestion`, assert the job's status is the suggestion and the outcome is `Applied`.
  - `dismiss_keeps_the_row_processed`: dismiss, then assert `processed_ids` still contains the id and `list_pending_reviews` is empty.
  - `pending_reviews_span_runs`: rows from two different runs with `PendingReview` are both listed.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml store::tests -- email`
Expected: FAIL, unresolved names.

- [ ] **Step 4: Implement** `store/email.rs` following the `store/companies.rs` pattern (`sqlx::query`, `from_row` helpers, `StoreError` for `NotFound`). Add the module and re-exports in `store/mod.rs`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml store::tests`
Expected: PASS, all store tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/migrations/0006_email_scan.sql src-tauri/src/store/email.rs src-tauri/src/store/mod.rs src-tauri/src/store/tests.rs
git commit -m "feat(store): add scan run and processed email tables

Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Mail account storage

**Files:**
- Create: `src-tauri/src/mail/mod.rs` (module root: `pub mod account; pub mod decide; pub mod classify; pub mod summary; pub mod imap; pub mod scan;` added as each task lands; `#[cfg(test)] mod tests;`)
- Create: `src-tauri/src/mail/account.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod mail;` with the same `#[allow(dead_code)]` convention as the other modules)
- Test: `src-tauri/src/mail/tests.rs`

**Interfaces:**
- Produces:
  - `struct MailAccount { host: String, port: u16, address: String, password: String }`
  - `account::load(store: &Store) -> Result<Option<MailAccount>>`: `None` unless all four values are present.
  - `account::save(store: &Store, a: &MailAccount) -> Result<()>`: writes the three settings and the secret.
  - `account::remove(store: &Store) -> Result<()>`: deletes the settings and the secret.
  - `account::validate(host: &str, port: u16, address: &str) -> Result<(), String>`: host non-empty, port non-zero, address contains `@`.
  - `account::normalize_password(p: &str) -> String`: removes all whitespace (Google displays app passwords with spaces).

- [ ] **Step 1: Write failing tests** in `mail/tests.rs`:
  - `save_then_load_round_trips`
  - `password_spaces_are_stripped_on_save`: save `"abcd efgh ijkl mnop"`, load, assert `password == "abcdefghijklmnop"`.
  - `load_is_none_when_incomplete`: save only the address, assert `load` returns `None`.
  - `remove_clears_everything`
  - `validate_rejects_bad_fields`: empty host, port 0, address without `@`.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml mail::tests`
Expected: FAIL, unresolved names.

- [ ] **Step 3: Implement** `account.rs`. Use `store.set_setting` / `get_setting` for the three keys and `set_secret` / `get_secret` / `delete_secret` for provider `gmail`.

- [ ] **Step 4: Run to verify pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml mail::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/mail src-tauri/src/lib.rs
git commit -m "feat(mail): store Gmail account settings and app password

Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>"
```

---

### Task 3: IMAP source and parsing

**Files:**
- Modify: `src-tauri/Cargo.toml` (add `async-imap` with the tokio runtime feature, `tokio-rustls`, `webpki-roots`, `mailparse`). Match the rustls version already in `Cargo.lock`. Confirm the versions with `cargo add` before editing by hand.
- Create: `src-tauri/src/mail/imap.rs`
- Test: `src-tauri/src/mail/tests.rs`

**Interfaces:**
- Produces:
  - `struct MailHeader { uid: u32, message_id: String, subject: String, sender: String, received_at: String }`
  - `enum MailError { Auth(String), Connection(String), Protocol(String) }`
  - `#[async_trait] trait MailSource: Send + Sync { async fn check(&self) -> Result<(), MailError>; async fn recent_headers(&self, since: NaiveDate) -> Result<Vec<MailHeader>, MailError>; async fn body_text(&self, uid: u32) -> Result<String, MailError>; }`
  - `struct ImapSource { account: MailAccount }` implements `MailSource`. Each method opens a fresh session, which avoids dropped-connection state across a long scan.
  - `pub fn parse_header(raw: &[u8], uid: u32) -> MailHeader`: decodes RFC 2047 subject and sender.
  - `pub fn extract_text(raw: &[u8]) -> String`: text/plain part, else `html2text` of the text/html part.
  - `pub fn chunk_uids(uids: &[u32]) -> Vec<Vec<u32>>`: chunks of 200.
  - `pub fn gmail_link(message_id: &str) -> String`: `https://mail.google.com/mail/u/0/#search/rfc822msgid%3A{encoded id}`, with the id stripped of `<` and `>` and percent-encoded. Verify this URL format against a real message before shipping.

- [ ] **Step 1: Write failing tests** in `mail/tests.rs`:
  - `html_only_email_yields_readable_text`: raw message with only `text/html`; assert the output contains the visible words and no tags. (Review Focus 1.)
  - `plain_part_preferred_over_html`
  - `rfc2047_subject_is_decoded`: `Subject: =?UTF-8?Q?Interview_invitation_=E2=9C=85?=`; assert the decoded subject contains "Interview invitation". (Review Focus 3.)
  - `chunk_uids_splits_at_200`: 450 uids produce chunks of 200, 200, 50.
  - `gmail_link_strips_brackets_and_encodes`

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml mail::tests`
Expected: FAIL.

- [ ] **Step 3: Implement** the pure functions first (`parse_header`, `extract_text`, `chunk_uids`, `gmail_link`), then the `ImapSource` network methods:
  - `check`: TLS connect, LOGIN, `EXAMINE "[Gmail]/All Mail"`, LOGOUT. Login failure maps to `Auth`.
  - `recent_headers`: `UID SEARCH SINCE <d-Mon-yyyy>`, then `UID FETCH` per chunk with `BODY.PEEK[HEADER.FIELDS (MESSAGE-ID SUBJECT FROM DATE)]`.
  - `body_text`: `UID FETCH <uid> BODY.PEEK[]`, then `extract_text`.

- [ ] **Step 4: Run unit tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml mail::tests`
Expected: PASS.

- [ ] **Step 5: Live check (manual, not in CI)**

Add an `#[ignore]` test named `live_gmail_headers` that reads `CVD_TEST_GMAIL_ADDRESS` and `CVD_TEST_GMAIL_APP_PASSWORD` from the environment. Run it with `cargo test --manifest-path src-tauri/Cargo.toml live_gmail_headers -- --ignored` and expect at least one header back.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/mail
git commit -m "feat(mail): read Gmail headers and bodies over IMAP

Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Decision logic

**Files:**
- Create: `src-tauri/src/mail/decide.rs`
- Test: `src-tauri/src/mail/tests.rs`

**Interfaces:**
- Produces:
  - `const AUTO_APPLY_THRESHOLD: f64 = 0.85;`
  - `const JOB_RELATED_THRESHOLD: f64 = 0.5;`
  - `struct Classified { job_related: f64, matched_job: Option<i64>, status_changed: f64, new_status: Option<(i64, f64)> }`
  - `enum Decision { Ignored, NoChange { job_id: i64 }, Applied { job_id: i64, status_id: i64, confidence: f64 }, PendingReview { job_id: i64, status_id: i64, confidence: f64 } }`
  - `fn decide(c: &Classified, current_status: Option<i64>) -> Decision`

Rules, in order:
1. `job_related < 0.5` or `matched_job` is `None`: `Ignored`.
2. `status_changed < 0.5`, or `new_status` is `None`, or the new status equals `current_status`: `NoChange`.
3. `status_changed >= 0.85` and new-status confidence `>= 0.85`: `Applied`.
4. Otherwise: `PendingReview`. The confidence reported is the new-status confidence.

- [ ] **Step 1: Write failing tests** covering each branch, plus the boundaries:
  - `job_related_exactly_half_counts`: 0.5 is related.
  - `job_related_just_below_half_is_ignored`: 0.499.
  - `applies_at_exact_threshold`: both 0.85 gives `Applied`.
  - `just_below_threshold_is_pending`: status 0.849 gives `PendingReview`.
  - `same_status_is_no_change`
  - `status_change_below_half_is_no_change`
  - `missing_match_is_ignored`

- [ ] **Step 2: Run to verify failure**, then **Step 3: Implement** `decide`. **Step 4: Run to verify pass.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml mail::tests::decide`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/mail/decide.rs src-tauri/src/mail/tests.rs src-tauri/src/mail/mod.rs
git commit -m "feat(mail): decide outcome from classification answers

Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>"
```

---

### Task 5: System One questions and classification

**Files:**
- Create: `src-tauri/src/mail/classify.rs`
- Test: `src-tauri/src/mail/tests.rs`

**Interfaces:**
- Consumes: `DecisionEngine::decide(&self, state: &str, questions: &BTreeMap<String, Question>)` and `Question::Noul { instructions }` / `Question::Choice { instructions, options }` from `engine`; `Answer::Noul { p }` and `Answer::Choice { choice, probabilities }`; `Classified` from Task 4.
- Produces:
  - `const BODY_CHARS: usize = 6000;`
  - `struct EmailInput { subject: String, sender: String, body: String }`: `body` is already truncated by `EmailInput::new(subject, sender, body)`, which truncates to `BODY_CHARS` on a char boundary.
  - `struct JobCtx { id: i64, company: String, title: String, url: String, status: Option<String> }`
  - `fn build_questions(jobs: &[JobCtx], statuses: &[JobStatus]) -> BTreeMap<String, Question>`: keys `job_related`, `matched_job`, `status_changed`, `new_status`. Choice option keys `job_<id>` plus `no_match`, and `status_<id>`. `instructions` for each choice is a JSON object `{"question": "...", "options": {"<key>": "<human label>"}}`, so the labels reach the model even though `Question::Choice` carries only option keys.
  - `fn build_state(email: &EmailInput, jobs: &[JobCtx]) -> String`: JSON string with `email` and `jobs`.
  - `async fn classify(engine: &dyn DecisionEngine, email: &EmailInput, jobs: &[JobCtx], statuses: &[JobStatus]) -> Result<Classified, EngineError>`. Missing or unparseable answers return `EngineError::BadResponse`.

- [ ] **Step 1: Write failing tests** in `mail/tests.rs`:
  - `questions_list_every_job_and_status_by_id`: keys `job_12`, `no_match`, `status_2` present.
  - `missing_company_still_gets_a_label` (Review Focus 4): a job with empty company and URL yields a non-empty human label that includes the title.
  - `long_body_is_truncated_on_char_boundary` (Review Focus 2): a body of 10,000 `ñ` characters truncates to 6000 characters without panicking.
  - `classify_parses_fake_answers`: `FakeDecisionEngine` returns the four answers. Assert `matched_job == Some(12)` and `new_status == Some((2, 0.93))`, where `0.93` is `probabilities["status_2"]`.
  - `unknown_option_is_bad_response`: the engine returns `matched_job` choice `job_999` for a job list without id 999. Assert `classify` returns `EngineError::BadResponse`. Unknown ids are never silently mapped.
  - `missing_answer_is_bad_response`

- [ ] **Step 2: Run to verify failure**, then **Step 3: Implement** the functions, then **Step 4: Run to verify pass.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml mail::tests::classify`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/mail/classify.rs src-tauri/src/mail/tests.rs src-tauri/src/mail/mod.rs
git commit -m "feat(mail): build System One questions and parse answers

Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>"
```

---

### Task 6: One-sentence summary via the chat model

**Files:**
- Create: `src-tauri/src/mail/summary.rs`
- Test: `src-tauri/src/mail/tests.rs` (uses `wiremock`, already a dev-dependency)

**Interfaces:**
- Consumes: `ChatEndpoint { base_url, api_key, model }` from `providers`; `EmailInput`, `JobCtx` from Task 5; `AppError` from `commands`.
- Produces:
  - `async fn summarize(client: &reqwest::Client, ep: &ChatEndpoint, email: &EmailInput, job: &JobCtx, previous: Option<&str>, next: &str) -> Result<String, AppError>`
  - POST `{ep.base_url}/chat/completions`. `Authorization: Bearer` when `api_key` is set. Body: `{ model, messages: [system, user] }`. The system message asks for exactly one plain-text sentence about the application update. The user message carries the subject, sender, job label, and body.
  - Reads `choices[0].message.content`, trims it, and rejects an empty string.
  - Errors: HTTP 401 or 403 maps to `AppError::ProviderAuth`, 402 maps to `ProviderCredits`, network failure maps to `ProviderUnavailable`, anything else maps to `EngineUnavailable`.

- [ ] **Step 1: Write failing tests** with wiremock:
  - `returns_trimmed_content`: the mock returns `"  Recruiter confirmed your interview.  "`; assert the trimmed sentence.
  - `unauthorized_maps_to_provider_auth`
  - `empty_content_is_an_error`

- [ ] **Step 2: Run to verify failure**, then **Step 3: Implement**, then **Step 4: Run to verify pass.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml mail::tests::summary`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/mail/summary.rs src-tauri/src/mail/tests.rs src-tauri/src/mail/mod.rs
git commit -m "feat(mail): summarize job emails with the chat model

Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>"
```

---

### Task 7: Scan orchestration

**Files:**
- Create: `src-tauri/src/mail/scan.rs`
- Modify: `src-tauri/src/state.rs` (add `scan_running: Arc<AtomicBool>` to `AppState`, and `pub fn begin_scan(&self) -> Option<ScanGuard>`; `ScanGuard` clears the flag on drop)
- Test: `src-tauri/src/mail/tests.rs` (or a `scan_tests.rs` if the file gets long, following the repo's `*_tests.rs` convention)

**Interfaces:**
- Consumes: `MailSource`, `MailError`, `MailHeader` (Task 3); `decide`, `Decision` (Task 4); `classify`, `EmailInput`, `JobCtx` (Task 5); `summarize` (Task 6); `Store` methods from Task 1; `DecisionEngine`; `ChatEndpoint`; `Emitter`.
- Produces:
  - `struct ScanDeps<'a> { store: &'a Store, mail: &'a dyn MailSource, engine: &'a dyn DecisionEngine, chat: &'a ChatEndpoint, http: &'a reqwest::Client, system_one_model: &'a str, emit: &'a Emitter, today: NaiveDate }`
  - `struct ScanSummary { run_id: i64, checked: usize, job_related: usize, applied: usize, pending: usize, no_change: usize, failed: usize }`
  - `async fn run_scan(deps: ScanDeps<'_>) -> Result<ScanSummary, AppError>`

Behavior:
1. Read `list_jds` and `list_job_statuses`. Zero of either: return `AppError::Io("Add a job and a status first.")` without creating a run.
2. `create_scan_run`. Call `mail.check()`, then `mail.recent_headers(today - 30 days)`. A `MailError` finishes the run as `Failed` with the message, and returns the mapped error.
3. Skip each header whose `message_id` is in `processed_ids()`. Do not fetch its body.
4. For each new header, emit `scan://progress` with `{ checked, total }`:
   - `mail.body_text(uid)` failing: count as `failed`, do not record, continue.
   - Re-read jobs and statuses. Build `EmailInput` and call `classify`:
     - `EngineError::Unauthorized`, `InsufficientCredits`, `RateLimited`, `Network`, `ModelUnavailable`, `Unavailable`: fatal, stop the scan.
     - `BadResponse`, `ContextOverflow`: count as `failed`, do not record, continue.
   - `decide(...)`:
     - `Ignored`: record `ignored`.
     - `NoChange`, `PendingReview`, `Applied`: call `summarize`. A provider error (`ProviderAuth`, `ProviderCredits`, `ProviderUnavailable`) is fatal. Any other error counts as `failed` and is not recorded.
     - `Applied` and `NoChange` and `PendingReview` are recorded with `record_processed`. Pass `apply_status = Some(status_id)` only for `Applied`.
5. `finish_scan_run(Done, checked, failed)`. Emit `scan://finished` with `{ run_id, status: "done", error: null }`.
6. A fatal error: `finish_scan_run(Failed, error)`, emit `scan://finished` with `{ run_id, status: "failed", error }`, and return the error. Rows already recorded stay.

- [ ] **Step 1: Write failing tests** using a `FakeMail` (struct implementing `MailSource` with canned headers and bodies, plus a body-fetch counter), `FakeDecisionEngine`, and wiremock for the chat endpoint:
  - `skips_processed_messages_without_fetching_bodies`: pre-record one `message_id`, assert the body counter excludes it. (Review Focus 5 setup.)
  - `ignored_email_is_recorded_without_chat_call`
  - `applied_email_updates_job_status`
  - `pending_email_leaves_job_status_unchanged`
  - `no_change_email_gets_summary_and_outcome_no_change`
  - `bad_engine_response_is_retried_next_run`: first run fails the email, the second run processes it.
  - `fatal_engine_error_stops_scan_and_keeps_earlier_rows`: engine returns `Unauthorized` on the second email. Assert the run is `failed`, the first row remains, and the third email was not fetched. (Review Focus 5.)
  - `imap_check_failure_fails_run`
  - `zero_statuses_refuses_to_start`: no run row is created. (Review Focus 4.)
  - `begin_scan_is_single_flight` in `state.rs` tests: the second `begin_scan` returns `None` until the guard drops.

- [ ] **Step 2: Run to verify failure**, then **Step 3: Implement** `scan.rs` and the `state.rs` guard, then **Step 4: Run to verify pass.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml mail::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/mail src-tauri/src/state.rs
git commit -m "feat(mail): orchestrate the 30-day Gmail scan

Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Tauri commands and events

**Files:**
- Modify: `src-tauri/src/commands.rs` (plain functions plus `ipc` wrappers, following the existing pattern)
- Modify: `src-tauri/src/lib.rs` (register the new `ipc::` handlers in `generate_handler!`)
- Modify: `src-tauri/src/commands.rs` `AppError` (add `MailAuth(String)` and `MailUnavailable(String)`, with `kind()` values `MailAuth` and `MailUnavailable`, and `From<MailError>`)
- Test: `src-tauri/src/commands_tests.rs`

**Interfaces:**
- Produces:
  - `struct MailAccountView { host: String, port: u16, address: String, has_password: bool }`. The password never leaves Rust.
  - `get_mail_account(st) -> Res<Option<MailAccountView>>`
  - `save_mail_account(st, host, port, address, password: Option<String>) -> Res<MailAccountView>`: `password: None` keeps the stored one. Calls `validate`. Requires a password when none is stored.
  - `delete_mail_account(st) -> Res<()>`
  - `test_mail_account(st, host, port, address, password: Option<String>) -> Res<()>`: uses `ImapSource::check`. Errors map `Auth` to `MailAuth`, and other `MailError`s to `MailUnavailable`.
  - `start_scan(st) -> Res<i64>`: returns the run id after creating it. The work runs in `tauri::async_runtime::spawn`, which owns a clone of `Store`, the `ImapSource`, the engine from `st.engine_for(active_model)`, the chat endpoint from `st.research_env().endpoint()`, a `reqwest::Client`, the emitter, and the `ScanGuard`. A second start while running returns `AppError::Io("A scan is already running.")`.
  - `get_scan_status(st) -> Res<ScanState>`: `struct ScanState { running: bool }` from `scan_running`. Named `ScanState` to avoid clashing with the store's `ScanStatus` enum.
  - `list_scan_results(st, run_id: i64) -> Res<Vec<ProcessedEmail>>`
  - `list_pending_reviews(st) -> Res<Vec<ProcessedEmail>>`
  - `apply_suggestion(st, message_id: String) -> Res<ProcessedEmail>`
  - `dismiss_suggestion(st, message_id: String) -> Res<()>`
  - Gmail link: `gmail_link(message_id)` is exposed through `ProcessedEmail` by adding a computed `gmail_url` field on the serialized view, so the frontend never builds it.

- [ ] **Step 1: Write failing tests** in `commands_tests.rs`:
  - `save_mail_account_rejects_bad_port`: port 0 gives `AppError::Io`.
  - `save_mail_account_keeps_password_when_none`
  - `get_mail_account_never_returns_the_password`: assert the view has `has_password: true` and no password field exists.
  - `start_scan_refuses_second_concurrent_start`: hold a `ScanGuard` and assert `start_scan` returns an error.
  - `apply_suggestion_unknown_message_is_not_found`

- [ ] **Step 2: Run to verify failure**, then **Step 3: Implement** the functions and wrappers, then register them in `lib.rs`. **Step 4: Run to verify pass.**

Run: `cargo test --manifest-path src-tauri/Cargo.toml commands_tests`
Expected: PASS.

- [ ] **Step 5: Full Rust check**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS, all tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/commands_tests.rs src-tauri/src/lib.rs
git commit -m "feat(commands): expose Gmail account, scan, and review commands

Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Settings Integrations card

**Files:**
- Modify: `src/app/core/ipc/types.ts` (add `MailAccountView`, `ScanState`, `ProcessedEmail`, `Outcome`; extend `AppErrorKind` with `MailAuth` and `MailUnavailable`)
- Modify: `src/app/core/ipc/errors.ts` (add the two kinds to `KINDS`)
- Modify: `src/app/core/ipc/commands.ts` (wrappers for the account and scan commands, using the existing `call<T>` helper)
- Create: `src/app/features/settings/gmail-card.component.ts`
- Modify: `src/app/features/settings/settings-page.component.ts` (add an "Integrations" heading and `<app-gmail-card />` below Preferences)
- Test: `src/app/features/settings/gmail-card.spec.ts`

**Interfaces:**
- Consumes: the Rust commands from Task 8.
- Produces: `GmailCardComponent` (selector `app-gmail-card`), which shows "Not connected" or "Connected as <address>". Fields: host, port, address, app password with a Show toggle. Buttons: Test connection, Save, Remove.

Behavior and copy:
- Test connection calls `testMailAccount` and shows a toast with the error message on failure.
- Save calls `saveMailAccount`. An empty password field sends `null`, which keeps the stored one.
- Remove calls `deleteMailAccount` after a confirmation.
- Connected status uses the emerald classes from the approved sketch: `bg-emerald-100 text-emerald-800 dark:bg-emerald-900/50 dark:text-emerald-200`.

- [ ] **Step 1: Write failing specs**:
  - shows "Not connected" when `getMailAccount` returns `null`
  - shows "Connected as diego@gmail.com" when an account exists
  - Save sends `null` for the password when the field is empty
  - Test connection shows the error toast with the server's message

- [ ] **Step 2: Run to verify failure**

Run: `bun run test -- gmail-card`
Expected: FAIL.

- [ ] **Step 3: Implement** the types, wrappers, component, and settings page change. Follow `provider-key.component.ts` for the form pattern and `ZardSonnerService` for toasts.

- [ ] **Step 4: Run to verify pass**

Run: `bun run test -- gmail-card`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app/core src/app/features/settings
git commit -m "feat(settings): add Gmail integration card

Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Shared scan store

**Files:**
- Create: `src/app/core/stores/scan.store.ts`
- Modify: `src/app/core/ipc/events.ts` (add listeners for `scan://progress` and `scan://finished`, following the file's existing listen helper; if none exists, add one with `listen` from `@tauri-apps/api/event`)
- Test: `src/app/core/stores/scan.store.spec.ts`

**Interfaces:**
- Produces: `ScanStore` (`providedIn: 'root'`) with signals:
  - `running`: boolean, rehydrated from `getScanStatus()` on init
  - `progress`: `{ checked, total } | null`
  - `finishedRunId`: number | null, set on `scan://finished`, cleared by `acknowledgeFinished()`
  - `lastError`: string | null
  - `finishedSummary`: the `scan://finished` payload minus `status` and `error`, or null
  - `pendingCount`: number, from `listPendingReviews().length`
- Methods: `start(): Promise<void>` (calls `startScan`, sets `running`), `refreshPending(): Promise<void>`, `acknowledgeFinished(): void`.

- [ ] **Step 1: Write failing specs** with the listen helper mocked:
  - a `scan://progress` event updates `progress`
  - `scan://finished` with status `done` sets `running` false, `finishedRunId`, and refreshes `pendingCount`
  - `scan://finished` with status `failed` sets `lastError`
  - `start()` while already running is a no-op

- [ ] **Step 2: Run to verify failure**, then **Step 3: Implement**, then **Step 4: Run to verify pass.**

Run: `bun run test -- scan.store`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app/core/stores/scan.store.ts src/app/core/stores/scan.store.spec.ts src/app/core/ipc/events.ts
git commit -m "feat(ui): add scan store with progress and finish events

Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>"
```

---

### Task 11: Jobs page button and review badge

**Files:**
- Modify: `src/app/features/jobs/jobs-page.component.ts` (header actions: the scan button, then the review badge, next to "New job")
- Test: `src/app/features/jobs/jobs.spec.ts` (add cases)

**Interfaces:**
- Consumes: `ScanStore` from Task 10; `ScanResultsDialog` opener from Task 12.
- Behavior:
  - Button label: "Read emails for updates". While `running`: disabled, spinner, and "Reading emails {checked} / {total}".
  - Not connected (`getMailAccount()` is `null`): disabled, with the hint text "Connect Gmail in Settings › Integrations" as a router link to `/settings`.
  - Review badge: "{pendingCount} to review" when `pendingCount > 0`. Always clickable, including while a scan runs. Opens the dialog in review mode.
  - When `finishedRunId` becomes set, open the dialog once, then call `acknowledgeFinished()`.

- [ ] **Step 1: Write failing specs**:
  - button is disabled while running and shows the count
  - button is disabled with the connect hint when no account exists
  - the badge shows the pending count and is enabled while running
  - finishing a scan opens the dialog once

- [ ] **Step 2: Run to verify failure**, then **Step 3: Implement**, then **Step 4: Run to verify pass.**

Run: `bun run test -- jobs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app/features/jobs
git commit -m "feat(jobs): add Read emails for updates button and review badge

Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>"
```

---

### Task 12: Results popup

**Files:**
- Create: `src/app/features/jobs/scan-results-dialog.component.ts`
- Test: `src/app/features/jobs/scan-results-dialog.spec.ts`

**Interfaces:**
- Consumes: `listScanResults(runId)`, `listPendingReviews()`, `applySuggestion`, `dismissSuggestion` from Task 8; `ScanStore`; `StatusPillComponent` and `PILL_CLASSES` from `status-pill.component.ts`; `JobStatusesStore`; `jobLabel` from `jobs.store.ts`; the open-URL helper from `job-url.ts` (`openJobUrl`, or a sibling `openExternal` if the opener call should not be job-specific).
- Produces: a dialog with three sections, as in the approved sketch:
  - **Applied:** `applied` rows for the finished run. Row: job label, summary, previous status pill → new status pill, link icon.
  - **Needs review:** all `pending_review` rows. Row: job label, summary, suggested status pill, confidence as a percentage, Apply, Dismiss, link icon. Apply and Dismiss remove the row from the list on success.
  - **No change:** `no_change` rows for the finished run. Row: job label, summary, current status pill, the text "No change", link icon.
- Header line: "{checked} emails from the last 30 days · {job_related} job-related · {pending} need review · {no_change} no change". Counts come from `finishedSummary` in `ScanStore` (the `scan://finished` payload). If the dialog is opened from the badge rather than right after a scan, the header shows only the pending count.
- Empty state: "No job updates found in the last 30 days."
- Failed run: show `lastError` and "Check your connection in Settings › Integrations" for `MailAuth`, or the AI settings link for `ProviderAuth`.
- Rows are rendered as `<li>` items in one divided list, not cards, matching the approved sketch.

- [ ] **Step 1: Write failing specs**:
  - renders Applied, Needs review, and No change sections with the right rows
  - Apply calls `applySuggestion` and removes the row
  - Dismiss calls `dismissSuggestion` and removes the row
  - empty state text when there are no rows (Review Focus 4 covers rows without company or URL; assert the label falls back)
  - the link icon has an accessible label "Open original email in Gmail"

- [ ] **Step 2: Run to verify failure**, then **Step 3: Implement** the dialog, then **Step 4: Run to verify pass.**

Run: `bun run test -- scan-results-dialog`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app/features/jobs
git commit -m "feat(jobs): add scan results popup with review actions

Co-Authored-By: Claude Haiku 5.5 <noreply@anthropic.com>"
```

---

### Task 13: Full verification

**Files:** none changed unless a check fails.

- [ ] **Step 1: Rust**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS.

- [ ] **Step 2: Frontend**

Run: `bun run test`
Expected: PASS.

- [ ] **Step 3: Lint and type check** (use the scripts in `package.json`)

Run: `bun run build`
Expected: builds without type errors.

- [ ] **Step 4: Manual run**

Run `bun run tauri dev`. Connect a Gmail test account in Settings › Integrations, press "Read emails for updates", and confirm:
- the button disables and the count advances
- the popup opens and lists at least one job-related email, or the empty state
- Apply updates the job's status on the Jobs page
- the link icon opens the correct Gmail message (verifies the `gmail_link` format from Task 3)

- [ ] **Step 5: Report** the results of each check with the actual output. If a step failed, report the failure and do not claim success.
