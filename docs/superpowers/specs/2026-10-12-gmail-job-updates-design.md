# Gmail job updates: design

## Goal

When the user presses "Read emails for updates" on the Jobs page, CVD reads the last 30 days of mail, finds emails about tracked job applications, updates their statuses when confident, and shows a summary of every change. Mail is read over IMAP with an app password. Each email is processed once.

## Decisions

- **Mail source:** IMAP (`imap.gmail.com:993`, TLS) with a Gmail app password. Not SMTP, which only sends. Not the Gmail API, which requires Google's restricted-scope verification to distribute.
- **Models:** the System One model and the chat LLM are configured on the Settings page only, through the existing role configuration.
- **Trigger:** a button press. Nothing runs on app launch.
- **Scope:** `[Gmail]/All Mail`, all mail from the last 30 days, read or unread. Gmail's read flag is ignored.
- **Processing:** only emails absent from `processed_email` are processed. Gmail's read flag plays no role.
- **Confidence threshold:** 0.85.
- **Sequencing:** the scan runs in the Rust backend and continues if the user navigates away. Emails are processed one at a time.

## Flow and UI

- **Button:** "Read emails for updates" on the Jobs page. While a scan runs it is disabled and shows a spinner with a live count ("Reading emails 14 / 60"). The disabled state comes from a shared store, so it holds on every page.
- **Review badge:** "N to review" sits next to the button. It is always clickable and reopens the popup with pending reviews.
- **Popup:** opens over the current page when the scan ends. Closing it returns the user to where they were.
  - **Header:** totals, for example "60 emails from the last 30 days · 4 job-related · 2 need review."
  - **Applied:** this run's auto-applied changes. Each row shows the job, the one-sentence summary, the status change (old → new, using the existing status pill colors), and a link icon to the original email.
  - **Needs review:** every `pending_review` row across all runs, until handled. Each row has the summary, the suggested status, the confidence, Apply, Dismiss, and the link icon.
  - Each job is a row in a divided list, not a card. Status labels use the same classes as the jobs list (`PILL_CLASSES` in `status-pill.component.ts`).
- **Empty state:** if nothing job-related was found, the popup says so.
- **Errors:** a fatal error opens the popup with the error and the next step.

## Mail retrieval

- Connect with the settings in the mail account section (IMAP host, port, address), and read the app password from the existing secrets storage.
- Select `[Gmail]/All Mail` and run `UID SEARCH SINCE <today − 30 days>`.
- For each UID, fetch `X-GM-MSGID` and `Message-ID`. If `X-GM-MSGID` is already in `processed_email`, skip the message. Do not fetch the body.
- For unprocessed messages, fetch headers and the text body with `BODY.PEEK[]`, so nothing is marked as read.
- Build the Gmail link from `Message-ID`, using the Gmail search form `#search/rfc822msgid:<id>`. Verify this format before implementing.

## Per-email pipeline

Each unprocessed email runs these steps in order:

1. **Read the current jobs and statuses from the DB.** Do this fresh for each email, so nothing is cached. Build the `state` JSON from the email (sender, subject, body truncated to a fixed limit) and the jobs list (`id`, company, title, URL, current status).
2. **One System One call** to `POST {base}/v1/systemone` with four questions:
   - `job_related` (`noul`): is this about a job application?
   - `matched_job` (`choice`): options `job_<id>` for each tracked job, plus `no_match`.
   - `status_changed` (`noul`): does this email change the status of the matched job?
   - `new_status` (`choice`): options `status_<id>` for each label, showing what the job's status should be now.
3. **Decide the outcome:**
   - `job_related` probability below 0.5, or `matched_job` = `no_match`: outcome `ignored`. No summary call is made.
   - `status_changed` false, or `new_status` equals the current status: outcome `no_change`. Make the summary call and record it.
   - Both `status_changed` probability and `new_status` confidence at or above 0.85: outcome `applied`. Set `job_description.status_id` to the new status in the same transaction that writes the row.
   - Otherwise: outcome `pending_review`, with the suggested status and confidence stored. The job is not changed.
4. **Summary (matched emails only):** one call to the configured chat LLM for a one-sentence summary. The input is the email subject, the body (truncated), the job, and the old and new statuses. The summary is stored on the row.
5. **Record the row** in `processed_email`, linked to the current `scan_run`.

Each email is recorded only after its last step succeeds. The row and any status change are written in one transaction.

## Apply and Dismiss

- **Apply:** sets `status_id` to the stored suggested status and changes the outcome to `applied`. It applies even if the user changed the job's status by hand after the email arrived.
- **Dismiss:** sets the outcome to `dismissed`. The email is never processed again.

## Errors

- **Fatal** (IMAP login or connection failure, System One unauthorized, out of credits, or unavailable): stop the scan, set `scan_run.status = failed` with the message, and open the popup with the error and what to do about it. Rows already written stay as they are.
- **Per-email** (body fetch failure, bad System One response, summary call failure): skip the email, do not record it, count it as failed in the popup, and retry on the next run.
- **Concurrency:** one scan at a time. A second press while a scan runs is not possible, because the button is disabled.
- **App closed mid-scan:** the scan stops. Rows already recorded stay. Unprocessed emails are picked up next run.

## Data model

Migration `0006_email_scan.sql`:

- `scan_run`: `id`, `started_at`, `finished_at`, `status` (`running`, `done`, `failed`), `error`, `checked` (count of emails in the window), `failed` (count of per-email failures).
- `processed_email`: `gmail_msg_id` (TEXT, primary key), `message_id` (TEXT), `run_id` (references `scan_run`), `processed_at`, `received_at`, `subject`, `sender`, `matched_job_id` (nullable, references `job_description`), `suggested_status_id` (nullable, references `job_status`), `confidence` (REAL, nullable), `summary` (TEXT, nullable), `outcome` (`ignored`, `no_change`, `applied`, `pending_review`, `dismissed`).

The popup's "Applied" list shows this run's `applied` rows. "Needs review" shows all `pending_review` rows.

## Testing

- **Decision logic:** unit tests covering each outcome branch and the threshold boundary (0.85 exactly, just below).
- **Pipeline:** integration tests with a fake IMAP source and the existing fake System One engine (`engine/fake.rs`). Cover: already-processed skip, per-email failure retry, fatal error stopping the run, and the status update in one transaction.
- **Migration:** verify `0006` applies on the existing schema.
- **Frontend:** Angular specs for the button states (idle, running, review badge), and for Apply and Dismiss.

## Out of scope

- Multiple mail accounts
- Gmail API
- Running on app launch
- Storing email bodies
- Deleting `processed_email` rows

## To verify before implementation

- The Gmail web link format for `rfc822msgid`.
- Whether the System One `state` field accepts a JSON-encoded string, as the current wire code sends it. The OpenRouter docs also show an object form.
- That Google still allows app passwords for IMAP on the account.
