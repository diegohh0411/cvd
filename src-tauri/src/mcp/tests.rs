use super::validate::normalize_entry;
use super::CvServer;
use crate::store::types::{EntryInput, EntryKind};
use crate::store::Store;

fn entry(start: &str, end: Option<&str>) -> EntryInput {
    EntryInput {
        id: None,
        kind: EntryKind::Project,
        title: "CV app".into(),
        org: "Personal".into(),
        location: String::new(),
        start_date: start.into(),
        end_date: end.map(Into::into),
        description: String::new(),
        sort_hint: 0,
        archived: false,
        bullets: vec!["Built it".into()],
        skills: vec!["Rust".into()],
    }
}

async fn server() -> CvServer {
    CvServer::new(Store::open("sqlite::memory:").await.unwrap())
}

#[test]
fn rejects_bad_date() {
    assert!(normalize_entry(entry("Jan 2022", None)).unwrap_err().contains("YYYY-MM"));
}

#[test]
fn rejects_month_out_of_range() {
    assert!(normalize_entry(entry("2022-13", None)).is_err());
}

#[test]
fn rejects_end_before_start() {
    assert!(normalize_entry(entry("2023-01", Some("2022-01"))).is_err());
}

#[test]
fn rejects_blank_title() {
    let mut e = entry("2022-01", None);
    e.title = "  ".into();
    assert!(normalize_entry(e).is_err());
}

#[test]
fn trims_and_drops_empty_skills() {
    let mut e = entry("2022-01", None);
    e.skills = vec![" Rust ".into(), "".into()];
    e.bullets = vec![" a ".into(), " ".into()];
    let n = normalize_entry(e).unwrap();
    assert_eq!(n.skills, vec!["Rust"]);
    assert_eq!(n.bullets, vec!["a"]);
}

#[tokio::test]
async fn insert_then_duplicate_is_rejected_with_id() {
    let srv = server().await;
    let r = srv.upsert_entry(entry("2022-01", None), false).await.unwrap();
    let err = srv.upsert_entry(entry("2022-01", None), false).await.unwrap_err();
    assert!(err.message.contains(&format!("id {}", r.id.unwrap())), "{}", err.message);
}

#[tokio::test]
async fn dry_run_writes_nothing() {
    let srv = server().await;
    assert_eq!(srv.upsert_entry(entry("2022-01", None), true).await.unwrap().id, None);
    assert!(srv.list_entries(true).await.unwrap().is_empty());
}

#[tokio::test]
async fn update_by_id_replaces_bullets() {
    let srv = server().await;
    let id = srv.upsert_entry(entry("2022-01", None), false).await.unwrap().id.unwrap();
    let mut e = entry("2022-01", None);
    e.id = Some(id);
    e.bullets = vec!["New".into()];
    srv.upsert_entry(e, false).await.unwrap();
    let list = srv.list_entries(true).await.unwrap();
    assert_eq!(list.len(), 1);
    let texts: Vec<_> = list[0].bullets.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(texts, vec!["New"]);
}

#[tokio::test]
async fn update_unknown_id_is_invalid_params() {
    let srv = server().await;
    let mut e = entry("2022-01", None);
    e.id = Some(999);
    assert!(srv.upsert_entry(e, false).await.is_err());
}
