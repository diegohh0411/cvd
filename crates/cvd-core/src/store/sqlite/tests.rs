use super::super::types::*;
use super::super::{InboxFilter, NewProcessed, Outcome, ScanSource};
use super::Store;

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
    let known = sqlx::migrate!("./migrations/sqlite").iter().count() as i64;
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

/// A job at `statuses[0]`, and a row auto-applied from `statuses[0]` to `statuses[1]`.

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
    let migration = include_str!("../../../migrations/sqlite/0010_inbox.sql");
    let backfill = migration
        .lines()
        .find(|l| l.starts_with("UPDATE processed_email SET read_at"))
        .unwrap();
    sqlx::query(backfill).execute(&s.pool).await.unwrap();

    let row = s.get_processed("msg-old").await.unwrap().unwrap();
    assert_eq!(row.read_at.as_deref(), Some(row.processed_at.as_str()));
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
    sqlx::raw_sql(include_str!(
        "../../../migrations/sqlite/0009_local_server.sql"
    ))
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
    sqlx::raw_sql(include_str!(
        "../../../migrations/sqlite/0009_local_server.sql"
    ))
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
    sqlx::raw_sql(include_str!(
        "../../../migrations/sqlite/0009_local_server.sql"
    ))
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

#[tokio::test]
async fn writes_bump_change_versions() {
    let s = store().await;
    let before = s.change_versions().await.unwrap();
    s.set_setting("k", "v").await.unwrap();
    let after = s.change_versions().await.unwrap();
    assert!(after["settings"] > before["settings"]);
    assert_eq!(after["job_status"], before["job_status"]);
}
