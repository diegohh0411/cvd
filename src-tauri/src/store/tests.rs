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
    let jd = s.create_jd("Dev", "Acme", "text").await.unwrap();
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
    let jd = s.create_jd("Dev", "Acme", "text").await.unwrap();
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
    let jd = s.create_jd("Dev", "Acme", "text").await.unwrap();
    let other = s.create_jd("Ops", "Acme", "text").await.unwrap();
    let a = s.create_run(jd, "clef-flash").await.unwrap();
    let b = s.create_run(jd, "clef-flash").await.unwrap();
    s.create_run(other, "clef-flash").await.unwrap();
    let ids: Vec<i64> = s.list_runs(jd).await.unwrap().iter().map(|r| r.id).collect();
    assert_eq!(ids, vec![b, a]);
}
