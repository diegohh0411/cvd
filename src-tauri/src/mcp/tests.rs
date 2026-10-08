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

#[test]
fn db_path_prefers_env_override() {
    assert_eq!(
        super::db_path_from(Some("/tmp/x.db".into())),
        Some(std::path::PathBuf::from("/tmp/x.db"))
    );
    let default = super::db_path_from(None).unwrap();
    assert!(default.ends_with("com.diego.cv-decisions/cv.db"), "{}", default.display());
}

fn job_args() -> super::UpsertJobArgs {
    super::UpsertJobArgs { id: None, title: None, company: None, description: None, url: None, status: None }
}

#[tokio::test]
async fn upsert_job_creates_with_default_status_then_moves_it() {
    let s = server().await;
    let job = s
        .upsert_job(super::UpsertJobArgs {
            title: Some("Dev".into()),
            company: Some("Acme".into()),
            description: Some("Rust".into()),
            url: Some(" https://acme.dev/jobs/1 ".into()),
            ..job_args()
        })
        .await
        .unwrap();
    assert_eq!(job.status.as_deref(), Some("Bookmarked"));
    assert_eq!(job.url, "https://acme.dev/jobs/1");
    let moved = s
        .upsert_job(super::UpsertJobArgs { id: Some(job.id), status: Some("applied".into()), ..job_args() })
        .await
        .unwrap();
    assert_eq!(moved.status.as_deref(), Some("Applied"));
    assert_eq!(moved.url, "https://acme.dev/jobs/1");
    assert_eq!(s.list_jobs().await.unwrap().len(), 1);
}

#[tokio::test]
async fn upsert_job_rejects_unknown_status() {
    let s = server().await;
    let err = s
        .upsert_job(super::UpsertJobArgs {
            title: Some("Dev".into()),
            description: Some("Rust".into()),
            status: Some("Ghosted".into()),
            ..job_args()
        })
        .await
        .unwrap_err();
    assert!(err.message.contains("Bookmarked"));
}

#[tokio::test]
async fn upsert_job_status_creates_and_renames() {
    use crate::store::types::StatusColor;
    let s = server().await;
    let st = s
        .upsert_job_status(super::UpsertJobStatusArgs { id: None, name: "Rejected".into(), color: StatusColor::Red })
        .await
        .unwrap();
    assert_eq!(st.position, 4);
    let renamed = s
        .upsert_job_status(super::UpsertJobStatusArgs { id: Some(st.id), name: "Declined".into(), color: StatusColor::Gray })
        .await
        .unwrap();
    assert_eq!((renamed.name.as_str(), renamed.color), ("Declined", StatusColor::Gray));
}

#[tokio::test]
async fn update_skill_renames_and_sets_category() {
    let s = server().await;
    s.upsert_entry(entry("2022-01", None), false).await.unwrap();
    let id = s.store.list_skills().await.unwrap()[0].id;
    let sk = s
        .update_skill(super::UpdateSkillArgs { id, name: " Rust (lang) ".into(), category: "Languages".into() })
        .await
        .unwrap();
    assert_eq!((sk.id, sk.name.as_str(), sk.category.as_str()), (id, "Rust (lang)", "Languages"));
}

#[tokio::test]
async fn update_skill_onto_existing_name_merges() {
    let s = server().await;
    let mut a = entry("2022-01", None);
    a.skills = vec!["Rust".into(), "Backend".into()];
    s.upsert_entry(a, false).await.unwrap();
    let mut b = entry("2023-01", None);
    b.skills = vec!["Backend".into()];
    s.upsert_entry(b, false).await.unwrap();
    let skills = s.store.list_skills().await.unwrap();
    let backend = skills.iter().find(|k| k.name == "Backend").unwrap().id;
    let rust = skills.iter().find(|k| k.name == "Rust").unwrap().id;
    let merged = s
        .update_skill(super::UpdateSkillArgs { id: backend, name: "rust".into(), category: String::new() })
        .await
        .unwrap();
    assert_eq!(merged.id, rust);
    assert_eq!(s.store.list_skills().await.unwrap().len(), 1);
    for e in s.list_entries(true).await.unwrap() {
        let names: Vec<_> = e.skills.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, vec!["rust"]);
    }
}

#[tokio::test]
async fn delete_skill_unlinks_and_rejects_unknown() {
    let s = server().await;
    s.upsert_entry(entry("2022-01", None), false).await.unwrap();
    let id = s.store.list_skills().await.unwrap()[0].id;
    s.delete_skill(id).await.unwrap();
    assert!(s.list_entries(true).await.unwrap()[0].skills.is_empty());
    assert!(s.delete_skill(id).await.is_err());
    assert!(s
        .update_skill(super::UpdateSkillArgs { id, name: "X".into(), category: String::new() })
        .await
        .is_err());
}
