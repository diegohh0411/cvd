use std::collections::BTreeMap;

use super::types::*;
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
    let texts: Vec<_> = e.bullets.iter().map(|b| (b.position, b.text.as_str())).collect();
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
    let ids: Vec<i64> = s.list_runs(jd).await.unwrap().iter().map(|r| r.id).collect();
    assert_eq!(ids, vec![b, a]);
}

#[tokio::test]
async fn find_entry_by_key_matches_case_insensitively() {
    let s = store().await;
    let id = s.upsert_entry(&input("Engineer", &[], &[])).await.unwrap();
    assert_eq!(
        s.find_entry_by_key(EntryKind::Experience, "engineer", "ACME", "2022-01").await.unwrap(),
        Some(id)
    );
    assert_eq!(
        s.find_entry_by_key(EntryKind::Project, "Engineer", "Acme", "2022-01").await.unwrap(),
        None
    );
}

#[tokio::test]
async fn file_db_uses_wal() {
    let dir = std::env::temp_dir().join(format!("cvd-wal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let s = Store::open(&format!("sqlite://{}", dir.join("t.db").display())).await.unwrap();
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&s.pool).await.unwrap();
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
    assert_eq!(n, known + 1, "external open must not apply migrations to an existing DB");
}

#[tokio::test]
async fn job_statuses_seeded_and_editable() {
    let s = store().await;
    let names: Vec<_> = s.list_job_statuses().await.unwrap().into_iter().map(|x| x.name).collect();
    assert_eq!(names, ["Bookmarked", "Applied", "Interviewing", "Offer"]);

    let id = s.create_job_status("Rejected", StatusColor::Red).await.unwrap();
    assert!(s.create_job_status("rejected", StatusColor::Gray).await.is_err(), "names are case-insensitive unique");
    assert!(s.create_job_status("  ", StatusColor::Gray).await.is_err());
    assert!(s.update_job_status(id, "Declined", StatusColor::Pink).await.unwrap());

    let mut ids: Vec<_> = s.list_job_statuses().await.unwrap().into_iter().map(|x| x.id).collect();
    ids.reverse();
    s.reorder_job_statuses(&ids).await.unwrap();
    assert_eq!(s.list_job_statuses().await.unwrap()[0].name, "Declined");
}

#[tokio::test]
async fn job_tracking_defaults_and_status_delete_clears() {
    let s = store().await;
    let first = s.list_job_statuses().await.unwrap()[0].id;
    let jd = s.create_jd("Dev", "Acme", "text", "https://x.dev", None).await.unwrap();
    let job = s.get_jd(jd).await.unwrap().unwrap();
    assert_eq!((job.status_id, job.url.as_str()), (Some(first), "https://x.dev"));

    s.delete_job_status(first).await.unwrap();
    assert_eq!(s.get_jd(jd).await.unwrap().unwrap().status_id, None);
    assert!(!s.update_jd_tracking(9999, None, "").await.unwrap());
}

#[tokio::test]
async fn secret_roundtrip() {
    let s = store().await;
    assert_eq!(s.get_secret("openrouter").await.unwrap(), None);
    s.set_secret("openrouter", "sk-or-1").await.unwrap();
    assert_eq!(s.get_secret("openrouter").await.unwrap().as_deref(), Some("sk-or-1"));
    s.set_secret("openrouter", "sk-or-2").await.unwrap();
    assert_eq!(s.get_secret("openrouter").await.unwrap().as_deref(), Some("sk-or-2"));
    s.delete_secret("openrouter").await.unwrap();
    assert_eq!(s.get_secret("openrouter").await.unwrap(), None);
}

fn profile(industry: &str) -> CompanyProfile {
    CompanyProfile {
        industry: industry.into(),
        tech_stack: vec!["Rust".into()],
        sources: vec![Source { url: "https://acme.com".into(), title: "Acme".into() }],
        ..Default::default()
    }
}

#[tokio::test]
async fn company_lookup_order() {
    let s = store().await;
    let by_name = s.create_company("Acme", "acme", None).await.unwrap();
    let by_domain = s.create_company("Other", "other", Some("acme.com")).await.unwrap();
    let hit = s.find_company(Some("acme.com"), "acme").await.unwrap().unwrap();
    assert_eq!(hit.id, by_domain.id);
    assert_eq!(s.find_company(None, "acme").await.unwrap().unwrap().id, by_name.id);
    s.add_alias("acme corp", by_name.id).await.unwrap();
    assert_eq!(s.find_company(None, "acme corp").await.unwrap().unwrap().id, by_name.id);
    assert!(s.find_company(None, "nobody").await.unwrap().is_none());
    assert_eq!(by_name.status, CompanyStatus::Pending);
}

#[tokio::test]
async fn save_research_clears_edit() {
    let s = store().await;
    let c = s.create_company("Acme", "acme", None).await.unwrap();
    s.mark_research_failed(c.id, "boom", "2030-01-01 00:00:00").await.unwrap();
    let edited = s.edit_company(c.id, &profile("Edited")).await.unwrap();
    assert!(edited.edited_at.is_some());
    assert_eq!(edited.profile.industry, "Edited");
    let saved = s.save_research(c.id, "Acme Inc", Some("acme.com"), &profile("Fintech"), "lfm").await.unwrap();
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
    s.mark_research_failed(c.id, "one", "2030-01-01 00:00:00").await.unwrap();
    s.mark_research_failed(c.id, "two", "2030-01-02 00:00:00").await.unwrap();
    let got = s.get_company(c.id).await.unwrap();
    assert_eq!(got.fail_count, 2);
    assert_eq!(got.last_error, "two");
    assert_eq!(got.status, CompanyStatus::Failed);
    assert_eq!(got.retry_after.as_deref(), Some("2030-01-02 00:00:00"));
}

#[tokio::test]
async fn merge_moves_aliases_and_jobs() {
    let s = store().await;
    let keep = s.create_company("Acme", "acme", Some("acme.com")).await.unwrap();
    let dup = s.create_company("Acme Labs", "acme labs", None).await.unwrap();
    s.add_alias("acmelabs", dup.id).await.unwrap();
    let jd = s.create_jd("Dev", "Acme Labs", "text", "", None).await.unwrap();
    s.set_jd_company(jd, Some(dup.id)).await.unwrap();
    s.merge_company(dup.id, keep.id).await.unwrap();
    assert!(s.get_company(dup.id).await.is_err());
    assert_eq!(s.find_company(None, "acmelabs").await.unwrap().unwrap().id, keep.id);
    assert_eq!(s.find_company(None, "acme labs").await.unwrap().unwrap().id, keep.id);
    assert_eq!(s.get_jd(jd).await.unwrap().unwrap().company_id, Some(keep.id));
}

#[tokio::test]
async fn run_records_company_researched_at() {
    let s = store().await;
    let jd = s.create_jd("Dev", "Acme", "text", "", None).await.unwrap();
    let run = s.create_run(jd, "clef-flash").await.unwrap();
    assert_eq!(s.get_run(run).await.unwrap().company_researched_at, None);
    s.set_run_company(run, Some("2026-10-08 10:00:00")).await.unwrap();
    assert_eq!(s.get_run(run).await.unwrap().company_researched_at.as_deref(), Some("2026-10-08 10:00:00"));
}
