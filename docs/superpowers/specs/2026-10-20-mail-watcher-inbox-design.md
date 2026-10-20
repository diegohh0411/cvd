# Mail watcher and inbox: design

## Goal

While the app is open, CVD watches Gmail on its own and classifies new mail as it arrives, with no button press. Results land in a new Inbox page instead of a popup, so the user is never interrupted. The Inbox is both the notification feed (what changed on its own) and the review queue (what needs a decision).

This builds on `2026-10-12-gmail-job-updates-design.md`. The per-email pipeline (System One classification, `decide`, summary, `processed_email` row) is unchanged except where noted.

## Decisions

- **Trigger:** IMAP IDLE on the scan folder (default `[Gmail]/All Mail`) while the app is open. A catch-up scan runs on every (re)connect. A "Check now" button remains for manual runs.
- **Cost:** System One and the summary LLM run only for emails not already in `processed_email`, plus re-checks (below). IDLE and the header search are the only recurring work.
- **Re-check on new job:** when a job is added, recent `ignored` emails that looked job-related are re-classified against the new job list.
- **Bodies are stored** for job-related emails only. This reverses the "Storing email bodies" out-of-scope line of the earlier spec.
- **Inbox content:** an activity feed of every job-related result (applied, needs review, no change) with unread state. `ignored` rows never appear.
- **No interruptions:** background results update a nav badge only. No popup, no toast.

## Mail watcher (`src-tauri/src/mail/watch.rs`)

A long-lived tokio task owned by `AppState`.

- **Lifecycle:** starts at app launch if a Gmail account is saved. `save_mail_account` restarts it with the new settings; `delete_mail_account` stops it. It stops when the app exits.
- **Loop:**
  1. Connect and select the scan folder (`EXAMINE`, read-only).
  2. Run a scan (re-check first if needed, see below), then the existing incremental `run_scan` with source `watch`.
  3. Enter IDLE. Leave IDLE on an `EXISTS` response or after 10 minutes (Gmail drops idle connections before the RFC 2177 29-minute limit).
  4. On `EXISTS`, go to 2. On the 10-minute timeout, re-enter IDLE without scanning.
- **Coalescing:** scans go through the existing scan slot (`AppState::begin_scan`). A new-mail signal that arrives while a scan runs sets a "dirty" flag; when the scan ends, one follow-up scan runs if the flag is set. Several signals never produce more than one queued scan. A manual "Check now" while the watcher is scanning is a no-op that reports the running scan.
- **Connection errors** (`MailError::Connection`, `Protocol`, IDLE dropped, sleep/wake): reconnect with exponential backoff from 5 s to 5 min, reset after a successful connect. A catch-up scan always follows a reconnect, so mail that arrived while disconnected is picked up.
- **Fatal errors** (`MailError::Auth`, `ProviderAuth`, `ProviderCredits`): the watcher enters `paused` with the error message and kind and stops retrying. It leaves `paused` when Gmail settings are saved, AI settings are saved, or the user presses Retry.
- `ProviderUnavailable` counts as a connection error (backoff), since it is usually transient.
- **IDLE connection:** the watcher owns its own IMAP session for IDLE. Scans keep opening their own sessions through `ImapSource`, as today.
- **Events:** `mail://watcher` with payload `{ state: "connecting" | "watching" | "scanning" | "paused" | "stopped", error, error_kind, last_checked_at }`. A `get_watcher_status` command returns the same shape for a page that opens late. The existing `scan://progress` and `scan://finished` events still fire for every scan.

## Re-check on new job

- **Detection:** jobs can be created from the app or from the separate `cvd-mcp` process, which writes to the same database. So the app does not rely on a creation hook. Instead the setting `mail_recheck_job_id` stores the highest job id already re-checked. Before each scan, if `MAX(job_description.id)` is greater, a re-check runs, then the setting is updated. `create_job` also nudges the watcher so a re-check runs right away instead of at the next new mail.
- **First run:** when the setting is absent it is initialised to the current max job id without re-checking, so upgrading does not re-classify the whole history.
- **Selection:** `processed_email` rows with `outcome = 'ignored'`, `received_at` within the last 30 days, and `job_related >= 0.5` or `job_related IS NULL` (rows recorded before this migration).
- **Processing:** each selected row goes through the per-email pipeline again using its stored `body`. Rows without a body (legacy) re-fetch it over IMAP with `UID SEARCH HEADER Message-ID <id>`. If the message is gone, the row is left as is.
- **Result:** the row is updated in place: `outcome`, `matched_job_id`, statuses, `confidence`, `summary`, `job_related`, `body`, `run_id` (the re-check run). If it is no longer `ignored`, `read_at` is cleared so it shows as unread. An auto-apply writes the status in the same transaction, as today.
- It runs inside the scan slot as a run with source `recheck`, so it never races a watch or manual scan.

## Stored bodies

- The stored text is exactly what the classifier sees: `EmailInput`'s body truncated to `BODY_CHARS` (6000).
- Stored only when `job_related >= 0.5`. Rows below that keep the score and a NULL body, and are never re-checked.
- Bodies are not shown anywhere except the Inbox row's expanded preview.
- `delete_mail_account` does not delete bodies; they are part of the job history like summaries.

## Inbox (`/inbox`)

- **Navigation:** "Inbox" is the first item in `app-sidebar.component.ts`. It shows a badge with the unread count, or a red dot when the watcher is `paused`.
- **Header:**
  - Watcher status line: "Watching Gmail · checked 2 min ago", "Checking 3 new emails…" (from `scan://progress`), "Connecting…", or "Paused: <error>" with a link to Settings › Integrations (mail errors) or AI › AI Models (provider errors) and a **Retry** button. When Gmail is not connected: "Connect Gmail in Settings › Integrations."
  - **Check now:** runs a scan with source `manual`. Disabled while a scan runs. On finish it shows one toast ("2 updates" or "No new updates"). This replaces "Read emails for updates" on the Jobs page.
  - **Mark all read.**
- **Filters:** tabs **All · Needs review · Unread**, kept in the `filter` query param.
- **Ordering:** `pending_review` rows first, then everything by `processed_at` descending. (`received_at` stores the raw RFC 2822 `Date` header, which does not sort as text. A re-matched email surfacing at the top is intended.)
- **Rows** use the popup's current layout (divided list, `StatusPillComponent`): job name, one-sentence summary, relative received time, unread dot, Gmail link icon.
  - **Applied:** old → new pills and **Undo**.
  - **Needs review:** suggested status pill, confidence, **Apply** and **Dismiss**.
  - **No change:** current status pill and "No change".
  - **Undone:** old → new pills struck through, "Undone".
- **Expanding a row** shows the stored body preview (plain text, scrollable) and marks the row read.
- **Undo:** allowed when the job's current `status_id` still equals the row's `suggested_status_id`. It sets the job back to `previous_status_id` and sets `undone_at`, in one transaction. Otherwise Undo is disabled with the tooltip "The status was changed after this update."
- **Live updates:** the page and the badge refresh on `scan://finished` and on a new `inbox://changed` event, emitted after a row is written or updated.

## Removed and changed UI

- `scan-results-dialog.component.ts` and its auto-open on `finishedRunId` are removed, and `ScanStore` drops `finishedRunId` and `acknowledgeFinished`.
- The Jobs page loses the scan button. Its "N to review" badge becomes a link to `/inbox?filter=review`.
- The workspace Emails tab is unchanged.

## Data model

Migration `0010_inbox.sql` (`0009` is taken by `0009_local_server.sql` from PR #48):

- `processed_email`:
  - `job_related REAL`: the System One `job_related` probability. NULL for rows recorded before this migration.
  - `body TEXT`: truncated body, NULL when `job_related < 0.5`.
  - `read_at TEXT`: NULL means unread. Backfilled to `processed_at` for existing rows so upgrading does not flood the badge.
  - `undone_at TEXT`.
- `scan_run.source TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('watch','recheck','manual'))`.
- Index on `processed_email(read_at)` for the unread count.

## Commands

- New: `get_watcher_status`, `retry_watcher`, `list_inbox(filter)`, `unread_count`, `mark_read(message_id)`, `mark_all_read`, `undo_applied(message_id)`.
- Changed: `start_scan` records source `manual`. `apply_suggestion` and `dismiss_suggestion` also set `read_at`.

## Errors

- Per-email failures behave as today: the email is not recorded and is retried on the next scan.
- A fatal error during a watch scan finishes the run as `failed` (as today) and pauses the watcher.
- A failed re-check leaves `mail_recheck_job_id` unchanged, so it is retried on the next scan.

## Testing

- **Watcher:** a fake IDLE source drives the state machine: catch-up on connect; several `EXISTS` signals during a scan produce exactly one follow-up scan; backoff timing on connection errors (with tokio's paused clock); `paused` on auth failure and resume on Retry; re-IDLE after 10 minutes without scanning.
- **Re-check:** selection (score threshold, NULL legacy rows, 30-day window); the watermark initialises without re-checking on first run and advances only on success; a re-matched row becomes unread; the legacy row re-fetch path.
- **Store:** Undo guard (allowed, blocked after a manual status change); `read_at` backfill; `0010` applies on top of `0009`.
- **Frontend:** Angular specs for the Inbox filters, ordering, read/unread, Apply/Dismiss/Undo, watcher status states, the Check now toast, and the sidebar badge and paused dot.

## Out of scope

- Watching while the app is closed (no background daemon, no launch at login)
- OS-level notifications
- Multiple mail accounts or folders
- Re-checking when a job is edited (only new jobs trigger it)
- Deleting stored bodies or `processed_email` rows
