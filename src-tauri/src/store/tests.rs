use std::collections::BTreeMap;

use super::types::*;
use super::{InboxFilter, NewProcessed, Outcome, ScanSource, Store};

async fn store() -> Store {
    Store::open("sqlite::memory:").await.unwrap()
}

fn input(title: &str, bullets: &[&str], skills: &[&str]) -> EntryInput {
    EntryInput {
        id: None,
        kind: EntryKind::Experience,
        title: title.into(),
        org: "Acme".into(),
        location: "Remote".into(),
        start_date: "2022-01".into(),
        end_date: None,
        description: String::new(),
        sort_hint: 0,
        archived: false,
        bullets: bullets.iter().map(|s| s.to_string()).collect(),
        skills: skills.iter().map(|s| s.to_string()).collect(),
    }
}

#[tokio::test]
async fn entry_roundtrip() {
    let s = store().await;
    s.upsert_entry(&input("Engineer", &["a", "b", "c"], &["Rust", "SQL"]))
        .await
        .unwrap();
    let entries = s.list_entries(false).await.unwrap();
    assert_eq!(entries.len(), 1);
    let e = &entries[0];
    assert_eq!(e.title, "Engineer");
    assert_eq!(e.kind, EntryKind::Experience);
    let texts: Vec<_> = e
        .bullets
        .iter()
        .map(|b| (b.position, b.text.as_str()))
        .collect();
    assert_eq!(texts, vec![(0, "a"), (1, "b"), (2, "c")]);
    let mut names: Vec<_> = e.skills.iter().map(|k| k.name.as_str()).collect();
    names.sort();
    assert_eq!(names, vec!["Rust", "SQL"]);
}

#[tokio::test]
async fn upsert_replaces_bullets() {
    let s = store().await;
    let id = s
        .upsert_entry(&input("Engineer", &["a", "b", "c"], &["Rust"]))
        .await
        .unwrap();
    let mut again = input("Engineer II", &["only"], &["Rust"]);
    again.id = Some(id);
    assert_eq!(s.upsert_entry(&again).await.unwrap(), id);
    let entries = s.list_entries(false).await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].title, "Engineer II");
    assert_eq!(entries[0].bullets.len(), 1);
    assert_eq!(entries[0].bullets[0].text, "only");
    assert_eq!(entries[0].skills.len(), 1);
}

#[tokio::test]
async fn skill_names_case_insensitive() {
    let s = store().await;
    s.upsert_entry(&input("A", &["x"], &["Go"])).await.unwrap();
    s.upsert_entry(&input("B", &["y"], &["go"])).await.unwrap();
    assert_eq!(s.list_skills().await.unwrap().len(), 1);
}

#[tokio::test]
async fn archived_hidden() {
    let s = store().await;
    s.upsert_entry(&input("Live", &["x"], &[])).await.unwrap();
    let mut old = input("Old", &["y"], &[]);
    old.archived = true;
    s.upsert_entry(&old).await.unwrap();
    assert_eq!(s.list_entries(false).await.unwrap().len(), 1);
    assert_eq!(s.list_entries(true).await.unwrap().len(), 2);
}

#[tokio::test]
async fn scores_roundtrip() {
    let s = store().await;
    let jd = s.create_jd("Dev", "Acme", "text", "", None).await.unwrap();
    let run = s.create_run(jd, "clef-flash").await.unwrap();
    let rows = vec![
        ScoreRow {
            run_id: run,
            target_type: TargetType::Entry,
            target_id: 1,
            question: "How relevant?".into(),
            probabilities: BTreeMap::from([("0".into(), 0.1), ("4".into(), 0.9)]),
            score: 3.6,
        },
        ScoreRow {
            run_id: run,
            target_type: TargetType::Skill,
            target_id: 2,
            question: "Needs Rust?".into(),
            probabilities: BTreeMap::from([("yes".into(), 0.75), ("no".into(), 0.25)]),
            score: 0.75,
        },
    ];
    s.insert_scores(&rows).await.unwrap();
    assert_eq!(s.scores_for_run(run).await.unwrap(), rows);
}

#[tokio::test]
async fn get_run_reflects_status() {
    let s = store().await;
    let jd = s.create_jd("Dev", "Acme", "text", "", None).await.unwrap();
    let run = s.create_run(jd, "clef-flash").await.unwrap();
    s.set_run_status(run, RunStatus::Cancelled).await.unwrap();
    let got = s.get_run(run).await.unwrap();
    assert!(!got.created_at.is_empty());
    assert_eq!(
        got,
        MatchRun {
            id: run,
            jd_id: jd,
            model_id: "clef-flash".into(),
            status: RunStatus::Cancelled,
            created_at: got.created_at.clone(),
            company_researched_at: None,
        }
    );
    assert!(s.get_run(run + 99).await.is_err());
}

#[tokio::test]
async fn delete_entry_cascades() {
    let s = store().await;
    let id = s
        .upsert_entry(&input("Engineer", &["a", "b"], &["Rust"]))
        .await
        .unwrap();
    s.delete_entry(id).await.unwrap();
    assert!(s.list_entries(true).await.unwrap().is_empty());
    for table in ["bullet", "entry_skill"] {
        let n: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&s.pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "{table} rows should cascade");
    }
}

#[tokio::test]
async fn settings_roundtrip() {
    let s = store().await;
    assert_eq!(s.get_setting("k").await.unwrap(), None);
    s.set_setting("k", "a").await.unwrap();
    s.set_setting("k", "b").await.unwrap();
    assert_eq!(s.get_setting("k").await.unwrap().as_deref(), Some("b"));
}

#[tokio::test]
async fn list_runs_newest_first_per_job() {
    let s = store().await;
    let jd = s.create_jd("Dev", "Acme", "text", "", None).await.unwrap();
    let other = s.create_jd("Ops", "Acme", "text", "", None).await.unwrap();
    let a = s.create_run(jd, "clef-flash").await.unwrap();
    let b = s.create_run(jd, "clef-flash").await.unwrap();
    s.create_run(other, "clef-flash").await.unwrap();
    let ids: Vec<i64> = s
        .list_runs(jd)
        .await
        .unwrap()
        .iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(ids, vec![b, a]);
}

#[tokio::test]
async fn find_entry_by_key_matches_case_insensitively() {
    let s = store().await;
    let id = s.upsert_entry(&input("Engineer", &[], &[])).await.unwrap();
    assert_eq!(
        s.find_entry_by_key(EntryKind::Experience, "engineer", "ACME", "2022-01")
            .await
            .unwrap(),
        Some(id)
    );
    assert_eq!(
        s.find_entry_by_key(EntryKind::Project, "Engineer", "Acme", "2022-01")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn file_db_uses_wal() {
    let dir = std::env::temp_dir().join(format!("cvd-wal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let s = Store::open(&format!("sqlite://{}", dir.join("t.db").display()))
        .await
        .unwrap();
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&s.pool)
        .await
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(mode, "wal");
}

#[tokio::test]
async fn open_external_tolerates_newer_schema() {
    let dir = std::env::temp_dir().join(format!("cvd-ext-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite://{}", dir.join("t.db").display());
    let app = Store::open(&url).await.unwrap();
    let known = sqlx::migrate!("./migrations").iter().count() as i64;
    // Simulate a migration from a newer app build that this binary doesn't know.
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES (9999, 'future', 1, x'00', 0)",
    )
    .execute(&app.pool)
    .await
    .unwrap();
    let ext = Store::open_external(&url).await;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&app.pool)
        .await
        .unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert!(ext.is_ok(), "{:?}", ext.err());
    assert_eq!(
        n,
        known + 1,
        "external open must not apply migrations to an existing DB"
    );
}

#[tokio::test]
async fn job_statuses_seeded_and_editable() {
    let s = store().await;
    let names: Vec<_> = s
        .list_job_statuses()
        .await
        .unwrap()
        .into_iter()
        .map(|x| x.name)
        .collect();
    assert_eq!(names, ["Bookmarked", "Applied", "Interviewing", "Offer"]);

    let id = s
        .create_job_status("Rejected", StatusColor::Red)
        .await
        .unwrap();
    assert!(
        s.create_job_status("rejected", StatusColor::Gray)
            .await
            .is_err(),
        "names are case-insensitive unique"
    );
    assert!(s.create_job_status("  ", StatusColor::Gray).await.is_err());
    assert!(s
        .update_job_status(id, "Declined", StatusColor::Pink)
        .await
        .unwrap());

    let mut ids: Vec<_> = s
        .list_job_statuses()
        .await
        .unwrap()
        .into_iter()
        .map(|x| x.id)
        .collect();
    ids.reverse();
    s.reorder_job_statuses(&ids).await.unwrap();
    assert_eq!(s.list_job_statuses().await.unwrap()[0].name, "Declined");
}

#[tokio::test]
async fn job_tracking_defaults_and_status_delete_clears() {
    let s = store().await;
    let first = s.list_job_statuses().await.unwrap()[0].id;
    let jd = s
        .create_jd("Dev", "Acme", "text", "https://x.dev", None)
        .await
        .unwrap();
    let job = s.get_jd(jd).await.unwrap().unwrap();
    assert_eq!(
        (job.status_id, job.url.as_str()),
        (Some(first), "https://x.dev")
    );

    s.delete_job_status(first).await.unwrap();
    assert_eq!(s.get_jd(jd).await.unwrap().unwrap().status_id, None);
    assert!(!s.update_jd_tracking(9999, None, "").await.unwrap());
}

#[tokio::test]
async fn job_description_text_is_editable() {
    let s = store().await;
    let jd = s.create_jd("Dev", "Acme", "old", "", None).await.unwrap();
    assert!(s
        .update_jd_text(jd, "## New\n\nBuild compilers.")
        .await
        .unwrap());
    assert_eq!(
        s.get_jd(jd).await.unwrap().unwrap().raw_text,
        "## New\n\nBuild compilers."
    );
    assert!(!s.update_jd_text(9999, "x").await.unwrap());
}

#[tokio::test]
async fn secret_roundtrip() {
    let s = store().await;
    assert_eq!(s.get_secret("openrouter").await.unwrap(), None);
    s.set_secret("openrouter", "sk-or-1").await.unwrap();
    assert_eq!(
        s.get_secret("openrouter").await.unwrap().as_deref(),
        Some("sk-or-1")
    );
    s.set_secret("openrouter", "sk-or-2").await.unwrap();
    assert_eq!(
        s.get_secret("openrouter").await.unwrap().as_deref(),
        Some("sk-or-2")
    );
    s.delete_secret("openrouter").await.unwrap();
    assert_eq!(s.get_secret("openrouter").await.unwrap(), None);
}

fn profile(industry: &str) -> CompanyProfile {
    CompanyProfile {
        industry: industry.into(),
        tech_stack: vec!["Rust".into()],
        sources: vec![Source {
            url: "https://acme.com".into(),
            title: "Acme".into(),
        }],
        ..Default::default()
    }
}

#[tokio::test]
async fn company_lookup_order() {
    let s = store().await;
    let by_name = s.create_company("Acme", "acme", None).await.unwrap();
    let by_domain = s
        .create_company("Other", "other", Some("acme.com"))
        .await
        .unwrap();
    let hit = s
        .find_company(Some("acme.com"), "acme")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hit.id, by_domain.id);
    assert_eq!(
        s.find_company(None, "acme").await.unwrap().unwrap().id,
        by_name.id
    );
    s.add_alias("acme corp", by_name.id).await.unwrap();
    assert_eq!(
        s.find_company(None, "acme corp").await.unwrap().unwrap().id,
        by_name.id
    );
    assert!(s.find_company(None, "nobody").await.unwrap().is_none());
    assert_eq!(by_name.status, CompanyStatus::Pending);
}

#[tokio::test]
async fn save_research_clears_edit() {
    let s = store().await;
    let c = s.create_company("Acme", "acme", None).await.unwrap();
    s.mark_research_failed(c.id, "boom", "2030-01-01 00:00:00")
        .await
        .unwrap();
    let edited = s.edit_company(c.id, &profile("Edited")).await.unwrap();
    assert!(edited.edited_at.is_some());
    assert_eq!(edited.profile.industry, "Edited");
    let saved = s
        .save_research(
            c.id,
            "Acme Inc",
            Some("acme.com"),
            &profile("Fintech"),
            "lfm",
        )
        .await
        .unwrap();
    assert_eq!(saved.edited_at, None);
    assert_eq!(saved.profile, profile("Fintech"));
    assert_eq!(saved.name, "Acme Inc");
    assert_eq!(saved.domain.as_deref(), Some("acme.com"));
    assert_eq!(saved.status, CompanyStatus::Ok);
    assert_eq!(saved.fail_count, 0);
    assert_eq!(saved.last_error, "");
    assert_eq!(saved.retry_after, None);
    assert!(saved.researched_at.is_some());
    assert_eq!(saved.model_id, "lfm");
}

#[tokio::test]
async fn mark_failed_increments() {
    let s = store().await;
    let c = s.create_company("Acme", "acme", None).await.unwrap();
    s.mark_research_failed(c.id, "one", "2030-01-01 00:00:00")
        .await
        .unwrap();
    s.mark_research_failed(c.id, "two", "2030-01-02 00:00:00")
        .await
        .unwrap();
    let got = s.get_company(c.id).await.unwrap();
    assert_eq!(got.fail_count, 2);
    assert_eq!(got.last_error, "two");
    assert_eq!(got.status, CompanyStatus::Failed);
    assert_eq!(got.retry_after.as_deref(), Some("2030-01-02 00:00:00"));
}

#[tokio::test]
async fn merge_moves_aliases_and_jobs() {
    let s = store().await;
    let keep = s
        .create_company("Acme", "acme", Some("acme.com"))
        .await
        .unwrap();
    let dup = s
        .create_company("Acme Labs", "acme labs", None)
        .await
        .unwrap();
    s.add_alias("acmelabs", dup.id).await.unwrap();
    let jd = s
        .create_jd("Dev", "Acme Labs", "text", "", None)
        .await
        .unwrap();
    s.set_jd_company(jd, Some(dup.id)).await.unwrap();
    s.merge_company(dup.id, keep.id).await.unwrap();
    assert!(s.get_company(dup.id).await.is_err());
    assert_eq!(
        s.find_company(None, "acmelabs").await.unwrap().unwrap().id,
        keep.id
    );
    assert_eq!(
        s.find_company(None, "acme labs").await.unwrap().unwrap().id,
        keep.id
    );
    assert_eq!(
        s.get_jd(jd).await.unwrap().unwrap().company_id,
        Some(keep.id)
    );
}

#[tokio::test]
async fn run_records_company_researched_at() {
    let s = store().await;
    let jd = s.create_jd("Dev", "Acme", "text", "", None).await.unwrap();
    let run = s.create_run(jd, "clef-flash").await.unwrap();
    assert_eq!(s.get_run(run).await.unwrap().company_researched_at, None);
    s.set_run_company(run, Some("2026-10-08 10:00:00"))
        .await
        .unwrap();
    assert_eq!(
        s.get_run(run)
            .await
            .unwrap()
            .company_researched_at
            .as_deref(),
        Some("2026-10-08 10:00:00")
    );
}

fn processed(message_id: &str, run_id: i64, outcome: Outcome) -> NewProcessed {
    NewProcessed {
        message_id: message_id.into(),
        run_id,
        received_at: "2026-10-12 09:00:00".into(),
        subject: "Application update".into(),
        sender: "jobs@example.com".into(),
        matched_job_id: None,
        previous_status_id: None,
        suggested_status_id: None,
        confidence: Some(0.9),
        summary: Some("The application was received.".into()),
        outcome,
        job_related: Some(0.9),
        body: None,
    }
}

#[tokio::test]
async fn record_then_list_run_results_round_trips_fields() {
    let s = store().await;
    let run = s.create_scan_run(ScanSource::Manual).await.unwrap();
    let row = processed("msg-applied", run, Outcome::Applied);
    s.record_processed(&row, None).await.unwrap();

    let got = s.list_run_results(run).await.unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].subject, row.subject);
    assert_eq!(got[0].summary, row.summary);
    assert_eq!(got[0].outcome, Outcome::Applied);
}

#[tokio::test]
async fn processed_ids_contains_every_recorded_message() {
    let s = store().await;
    let run = s.create_scan_run(ScanSource::Manual).await.unwrap();
    for id in ["msg-one", "msg-two"] {
        s.record_processed(&processed(id, run, Outcome::Ignored), None)
            .await
            .unwrap();
    }

    let ids = s.processed_ids().await.unwrap();
    assert!(ids.contains("msg-one"));
    assert!(ids.contains("msg-two"));
}

#[tokio::test]
async fn record_with_apply_status_updates_the_job() {
    let s = store().await;
    let statuses = s.list_job_statuses().await.unwrap();
    let status_a = statuses[0].id;
    let status_b = statuses[1].id;
    let job = s
        .create_jd("Engineer", "Acme", "text", "", Some(status_a))
        .await
        .unwrap();
    let run = s.create_scan_run(ScanSource::Manual).await.unwrap();
    let mut row = processed("msg-record-apply", run, Outcome::Applied);
    row.matched_job_id = Some(job);
    s.record_processed(&row, Some(status_b)).await.unwrap();

    assert_eq!(
        s.get_jd(job).await.unwrap().unwrap().status_id,
        Some(status_b)
    );
}

#[tokio::test]
async fn apply_suggestion_sets_status_and_outcome() {
    let s = store().await;
    let statuses = s.list_job_statuses().await.unwrap();
    let previous = statuses[0].id;
    let suggested = statuses[1].id;
    let job = s
        .create_jd("Engineer", "Acme", "text", "", Some(previous))
        .await
        .unwrap();
    let run = s.create_scan_run(ScanSource::Manual).await.unwrap();
    let mut row = processed("msg-suggestion", run, Outcome::PendingReview);
    row.matched_job_id = Some(job);
    row.previous_status_id = Some(previous);
    row.suggested_status_id = Some(suggested);
    s.record_processed(&row, None).await.unwrap();

    let got = s.apply_suggestion("msg-suggestion").await.unwrap();
    assert_eq!(
        s.get_jd(job).await.unwrap().unwrap().status_id,
        Some(suggested)
    );
    assert_eq!(got.outcome, Outcome::Applied);
}

#[tokio::test]
async fn dismiss_keeps_the_row_processed() {
    let s = store().await;
    let run = s.create_scan_run(ScanSource::Manual).await.unwrap();
    s.record_processed(&processed("msg-dismiss", run, Outcome::PendingReview), None)
        .await
        .unwrap();

    s.dismiss_suggestion("msg-dismiss").await.unwrap();
    assert!(s.processed_ids().await.unwrap().contains("msg-dismiss"));
    assert!(s.list_pending_reviews().await.unwrap().is_empty());
}

#[tokio::test]
async fn pending_reviews_span_runs() {
    let s = store().await;
    let first_run = s.create_scan_run(ScanSource::Manual).await.unwrap();
    let second_run = s.create_scan_run(ScanSource::Manual).await.unwrap();
    s.record_processed(
        &processed("msg-review-one", first_run, Outcome::PendingReview),
        None,
    )
    .await
    .unwrap();
    s.record_processed(
        &processed("msg-review-two", second_run, Outcome::PendingReview),
        None,
    )
    .await
    .unwrap();

    let rows = s.list_pending_reviews().await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|row| row.message_id == "msg-review-one"));
    assert!(rows.iter().any(|row| row.message_id == "msg-review-two"));
}

#[tokio::test]
async fn processed_email_serializes_outcomes_in_snake_case() {
    let s = store().await;
    let run = s.create_scan_run(ScanSource::Manual).await.unwrap();
    s.record_processed(
        &processed("msg-serialized", run, Outcome::PendingReview),
        None,
    )
    .await
    .unwrap();

    let row = s.get_processed("msg-serialized").await.unwrap().unwrap();
    let value = serde_json::to_value(row).unwrap();
    assert_eq!(value["outcome"], "pending_review");
}

/// A job at `statuses[0]`, and a row auto-applied from `statuses[0]` to `statuses[1]`.
async fn applied_row(s: &Store) -> (i64, i64, i64) {
    let statuses = s.list_job_statuses().await.unwrap();
    let (a, b) = (statuses[0].id, statuses[1].id);
    let job = s
        .create_jd("Engineer", "Acme", "text", "", Some(a))
        .await
        .unwrap();
    let run = s.create_scan_run(ScanSource::Watch).await.unwrap();
    let mut row = processed("msg-undo", run, Outcome::Applied);
    row.matched_job_id = Some(job);
    row.previous_status_id = Some(a);
    row.suggested_status_id = Some(b);
    s.record_processed(&row, Some(b)).await.unwrap();
    (job, a, b)
}

#[tokio::test]
async fn migration_marks_existing_rows_read() {
    let s = store().await;
    let run = s.create_scan_run(ScanSource::Manual).await.unwrap();
    s.record_processed(&processed("msg-old", run, Outcome::Applied), None)
        .await
        .unwrap();
    sqlx::query("UPDATE processed_email SET read_at = NULL")
        .execute(&s.pool)
        .await
        .unwrap();
    let migration = include_str!("../../migrations/0010_inbox.sql");
    let backfill = migration
        .lines()
        .find(|l| l.starts_with("UPDATE processed_email SET read_at"))
        .unwrap();
    sqlx::query(backfill).execute(&s.pool).await.unwrap();

    let row = s.get_processed("msg-old").await.unwrap().unwrap();
    assert_eq!(row.read_at.as_deref(), Some(row.processed_at.as_str()));
}

#[tokio::test]
async fn new_rows_are_unread_and_counted() {
    let s = store().await;
    let run = s.create_scan_run(ScanSource::Watch).await.unwrap();
    s.record_processed(&processed("msg-a", run, Outcome::Applied), None)
        .await
        .unwrap();
    s.record_processed(&processed("msg-i", run, Outcome::Ignored), None)
        .await
        .unwrap();

    assert_eq!(s.unread_count().await.unwrap(), 1);
    assert_eq!(
        s.get_processed("msg-a").await.unwrap().unwrap().read_at,
        None
    );
}

#[tokio::test]
async fn upsert_rematch_marks_unread() {
    let s = store().await;
    let run = s.create_scan_run(ScanSource::Watch).await.unwrap();
    s.record_processed(&processed("msg-r", run, Outcome::Ignored), None)
        .await
        .unwrap();
    s.mark_all_read().await.unwrap();
    sqlx::query("UPDATE processed_email SET read_at = '2026-01-01 00:00:00'")
        .execute(&s.pool)
        .await
        .unwrap();

    let recheck = s.create_scan_run(ScanSource::Recheck).await.unwrap();
    let mut row = processed("msg-r", recheck, Outcome::PendingReview);
    row.summary = Some("Now matched.".into());
    s.record_processed(&row, None).await.unwrap();

    let got = s.get_processed("msg-r").await.unwrap().unwrap();
    assert_eq!(got.outcome, Outcome::PendingReview);
    assert_eq!(got.run_id, recheck);
    assert_eq!(got.summary.as_deref(), Some("Now matched."));
    assert_eq!(got.read_at, None);
}

#[tokio::test]
async fn upsert_still_ignored_keeps_read() {
    let s = store().await;
    let run = s.create_scan_run(ScanSource::Watch).await.unwrap();
    s.record_processed(&processed("msg-k", run, Outcome::Ignored), None)
        .await
        .unwrap();
    sqlx::query("UPDATE processed_email SET read_at = '2026-01-01 00:00:00'")
        .execute(&s.pool)
        .await
        .unwrap();

    s.record_processed(&processed("msg-k", run, Outcome::Ignored), None)
        .await
        .unwrap();

    let got = s.get_processed("msg-k").await.unwrap().unwrap();
    assert_eq!(got.read_at.as_deref(), Some("2026-01-01 00:00:00"));
}

#[tokio::test]
async fn list_inbox_orders_review_first_and_hides_ignored() {
    let s = store().await;
    let run = s.create_scan_run(ScanSource::Watch).await.unwrap();
    for (id, outcome) in [
        ("msg-review", Outcome::PendingReview),
        ("msg-applied", Outcome::Applied),
        ("msg-ignored", Outcome::Ignored),
        ("msg-nochange", Outcome::NoChange),
    ] {
        s.record_processed(&processed(id, run, outcome), None)
            .await
            .unwrap();
    }
    sqlx::query("UPDATE processed_email SET processed_at = '2026-10-01 00:00:00' WHERE message_id = 'msg-review'")
        .execute(&s.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE processed_email SET processed_at = '2026-10-03 00:00:00' WHERE message_id = 'msg-nochange'")
        .execute(&s.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE processed_email SET processed_at = '2026-10-02 00:00:00' WHERE message_id = 'msg-applied'")
        .execute(&s.pool)
        .await
        .unwrap();

    let ids: Vec<String> = s
        .list_inbox(InboxFilter::All)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.message_id)
        .collect();
    assert_eq!(ids, ["msg-review", "msg-nochange", "msg-applied"]);

    let review = s.list_inbox(InboxFilter::Review).await.unwrap();
    assert_eq!(review.len(), 1);
    assert_eq!(review[0].message_id, "msg-review");
}

#[tokio::test]
async fn list_inbox_unread_filter() {
    let s = store().await;
    let run = s.create_scan_run(ScanSource::Watch).await.unwrap();
    for id in ["msg-1", "msg-2"] {
        s.record_processed(&processed(id, run, Outcome::Applied), None)
            .await
            .unwrap();
    }
    s.mark_read("msg-1").await.unwrap();

    let unread = s.list_inbox(InboxFilter::Unread).await.unwrap();
    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].message_id, "msg-2");
}

#[tokio::test]
async fn undo_restores_previous_status() {
    let s = store().await;
    let (job, a, _) = applied_row(&s).await;

    let got = s.undo_applied("msg-undo").await.unwrap();

    assert_eq!(s.get_jd(job).await.unwrap().unwrap().status_id, Some(a));
    assert!(got.undone_at.is_some());
}

#[tokio::test]
async fn undo_blocked_after_manual_status_change() {
    let s = store().await;
    let (job, _, _) = applied_row(&s).await;
    let c = s.list_job_statuses().await.unwrap()[2].id;
    s.update_jd_tracking(job, Some(c), "").await.unwrap();

    let err = s.undo_applied("msg-undo").await.unwrap_err();

    assert!(err
        .to_string()
        .contains("The status was changed after this update."));
    assert_eq!(s.get_jd(job).await.unwrap().unwrap().status_id, Some(c));
    assert_eq!(
        s.get_processed("msg-undo")
            .await
            .unwrap()
            .unwrap()
            .undone_at,
        None
    );
}

#[tokio::test]
async fn undo_twice_errors() {
    let s = store().await;
    applied_row(&s).await;
    s.undo_applied("msg-undo").await.unwrap();

    assert!(s.undo_applied("msg-undo").await.is_err());
}

#[tokio::test]
async fn apply_and_dismiss_mark_read() {
    let s = store().await;
    let statuses = s.list_job_statuses().await.unwrap();
    let job = s
        .create_jd("Engineer", "Acme", "text", "", Some(statuses[0].id))
        .await
        .unwrap();
    let run = s.create_scan_run(ScanSource::Watch).await.unwrap();
    let mut row = processed("msg-apply", run, Outcome::PendingReview);
    row.matched_job_id = Some(job);
    row.suggested_status_id = Some(statuses[1].id);
    s.record_processed(&row, None).await.unwrap();
    s.record_processed(&processed("msg-dismiss", run, Outcome::PendingReview), None)
        .await
        .unwrap();

    s.apply_suggestion("msg-apply").await.unwrap();
    s.dismiss_suggestion("msg-dismiss").await.unwrap();

    assert!(s
        .get_processed("msg-apply")
        .await
        .unwrap()
        .unwrap()
        .read_at
        .is_some());
    assert!(s
        .get_processed("msg-dismiss")
        .await
        .unwrap()
        .unwrap()
        .read_at
        .is_some());
}

#[tokio::test]
async fn recheck_candidates_selects_job_related_and_legacy() {
    let s = store().await;
    let run = s.create_scan_run(ScanSource::Watch).await.unwrap();
    let cases = [
        ("msg-related", Outcome::Ignored, Some(0.7)),
        ("msg-legacy", Outcome::Ignored, None),
        ("msg-unrelated", Outcome::Ignored, Some(0.2)),
        ("msg-applied", Outcome::Applied, Some(0.9)),
        ("msg-old", Outcome::Ignored, Some(0.7)),
    ];
    for (id, outcome, score) in cases {
        let mut row = processed(id, run, outcome);
        row.job_related = score;
        s.record_processed(&row, None).await.unwrap();
    }
    sqlx::query("UPDATE processed_email SET processed_at = datetime('now', '-40 days') WHERE message_id = 'msg-old'")
        .execute(&s.pool)
        .await
        .unwrap();

    let mut ids: Vec<String> = s
        .recheck_candidates()
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.message_id)
        .collect();
    ids.sort();
    assert_eq!(ids, ["msg-legacy", "msg-related"]);
}

#[tokio::test]
async fn max_job_id_is_zero_without_jobs() {
    let s = store().await;
    assert_eq!(s.max_job_id().await.unwrap(), 0);
    let job = s
        .create_jd("Engineer", "Acme", "text", "", None)
        .await
        .unwrap();
    assert_eq!(s.max_job_id().await.unwrap(), job);
}

#[tokio::test]
async fn undo_after_manual_apply_restores_the_status_at_apply_time() {
    let s = store().await;
    let statuses = s.list_job_statuses().await.unwrap();
    let (recorded, by_hand, suggested) = (statuses[0].id, statuses[1].id, statuses[2].id);
    let job = s
        .create_jd("Engineer", "Acme", "text", "", Some(recorded))
        .await
        .unwrap();
    let run = s.create_scan_run(ScanSource::Watch).await.unwrap();
    let mut row = processed("msg-manual", run, Outcome::PendingReview);
    row.matched_job_id = Some(job);
    row.previous_status_id = Some(recorded);
    row.suggested_status_id = Some(suggested);
    s.record_processed(&row, None).await.unwrap();
    s.update_jd_tracking(job, Some(by_hand), "").await.unwrap();

    s.apply_suggestion("msg-manual").await.unwrap();
    s.undo_applied("msg-manual").await.unwrap();

    assert_eq!(
        s.get_jd(job).await.unwrap().unwrap().status_id,
        Some(by_hand)
    );
}

#[tokio::test]
async fn recheck_that_stays_ignored_keeps_processed_at() {
    let s = store().await;
    let run = s.create_scan_run(ScanSource::Watch).await.unwrap();
    s.record_processed(&processed("msg-aging", run, Outcome::Ignored), None)
        .await
        .unwrap();
    sqlx::query("UPDATE processed_email SET processed_at = '2026-01-01 00:00:00'")
        .execute(&s.pool)
        .await
        .unwrap();

    s.record_processed(&processed("msg-aging", run, Outcome::Ignored), None)
        .await
        .unwrap();

    let got = s.get_processed("msg-aging").await.unwrap().unwrap();
    assert_eq!(got.processed_at, "2026-01-01 00:00:00");
}

#[tokio::test]
async fn apply_memory_remembers_repoints_and_forgets() {
    let s = Store::open("sqlite::memory:").await.unwrap();
    assert!(s.apply_memory().await.unwrap().is_empty());
    s.remember_label("nombre de pila", "first_name")
        .await
        .unwrap();
    s.remember_label("nombre de pila", "first_name")
        .await
        .unwrap();
    s.remember_label("apodo", "first_name").await.unwrap();
    s.remember_label("apodo", "full_name").await.unwrap();
    let m = s.apply_memory().await.unwrap();
    assert_eq!(
        m.get("nombre de pila").map(String::as_str),
        Some("first_name")
    );
    assert_eq!(m.get("apodo").map(String::as_str), Some("full_name"));
    s.forget_label("apodo").await.unwrap();
    assert!(!s.apply_memory().await.unwrap().contains_key("apodo"));
}

#[tokio::test]
async fn job_emails_are_scoped_to_the_job_newest_first() {
    let s = store().await;
    let job = s
        .create_jd("Engineer", "Acme", "text", "", None)
        .await
        .unwrap();
    let other = s
        .create_jd("Designer", "Beta", "text", "", None)
        .await
        .unwrap();
    let run = s.create_scan_run(ScanSource::Manual).await.unwrap();
    for (id, job_id, received) in [
        ("msg-old", job, "2026-10-01 09:00:00"),
        ("msg-new", job, "2026-10-10 09:00:00"),
        ("msg-other", other, "2026-10-05 09:00:00"),
    ] {
        let mut row = processed(id, run, Outcome::NoChange);
        row.matched_job_id = Some(job_id);
        row.received_at = received.into();
        s.record_processed(&row, None).await.unwrap();
    }

    let ids: Vec<_> = s
        .list_job_emails(job)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.message_id)
        .collect();
    assert_eq!(ids, ["msg-new", "msg-old"]);
}

#[tokio::test]
async fn job_views_seed_one_and_keep_the_last() {
    let s = store().await;
    let seeded = s.list_job_views().await.unwrap();
    assert_eq!(seeded.len(), 1);
    assert_eq!(seeded[0].layout, ViewLayout::Table);
    assert_eq!(seeded[0].filters, serde_json::json!([]));
    assert_eq!(seeded[0].sorts, serde_json::json!([]));

    let board = JobViewInput {
        name: " Pipeline ".into(),
        layout: ViewLayout::Board,
        filters: serde_json::json!([{ "field": "status", "op": "is", "value": [1] }]),
        sorts: serde_json::json!([]),
    };
    let id = s.create_job_view(&board).await.unwrap();
    let saved = s.get_job_view(id).await.unwrap().unwrap();
    assert_eq!(saved.name, "Pipeline");
    assert_eq!(saved.filters, board.filters);
    assert_eq!(saved.position, 1);

    let renamed = JobViewInput {
        name: "Board".into(),
        ..board.clone()
    };
    assert!(s.update_job_view(id, &renamed).await.unwrap());
    assert!(!s.update_job_view(9999, &renamed).await.unwrap());
    assert!(s
        .create_job_view(&JobViewInput {
            name: " ".into(),
            ..board
        })
        .await
        .is_err());

    s.delete_job_view(seeded[0].id).await.unwrap();
    assert!(s.delete_job_view(id).await.is_err(), "the last view stays");
    assert_eq!(s.list_job_views().await.unwrap().len(), 1);
}

#[tokio::test]
async fn migration_clears_legacy_selections_and_preserves_remote() {
    let store = Store::open("sqlite::memory:").await.unwrap();
    store
        .set_setting(crate::state::ACTIVE_MODEL_KEY, "clef-flash")
        .await
        .unwrap();
    store
        .set_setting(crate::state::RESEARCH_MODEL_KEY, "openrouter:chat:free")
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../../migrations/0009_local_server.sql"))
        .execute(&store.pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .get_setting(crate::state::ACTIVE_MODEL_KEY)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        store
            .get_setting(crate::state::RESEARCH_MODEL_KEY)
            .await
            .unwrap()
            .as_deref(),
        Some("openrouter:chat:free")
    );
    assert_eq!(
        store
            .get_setting("ui.local_server_migration")
            .await
            .unwrap()
            .as_deref(),
        Some("1")
    );
    store
        .set_setting("ui.local_server_migration", "0")
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../../migrations/0009_local_server.sql"))
        .execute(&store.pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .get_setting("ui.local_server_migration")
            .await
            .unwrap()
            .as_deref(),
        Some("0")
    );
    store
        .set_setting(crate::state::RESEARCH_MODEL_KEY, "lfm2.5-2.6b")
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../../migrations/0009_local_server.sql"))
        .execute(&store.pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .get_setting(crate::state::RESEARCH_MODEL_KEY)
            .await
            .unwrap(),
        None
    );
}
