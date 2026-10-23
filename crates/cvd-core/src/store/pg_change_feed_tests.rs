use super::{tests::postgres_store, *};
use std::time::Duration;

fn runtime_url() -> String {
    std::env::var("CVD_TEST_POSTGRES_URL").expect("set CVD_TEST_POSTGRES_URL")
}

async fn admin() -> sqlx::PgPool {
    let url =
        std::env::var("CVD_TEST_POSTGRES_ADMIN_URL").expect("set CVD_TEST_POSTGRES_ADMIN_URL");
    sqlx::PgPool::connect(&url).await.unwrap()
}

async fn next(rx: &mut ChangeReceiver) -> Vec<String> {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("no change notification")
        .unwrap()
}

/// Unions batches until every table in `want` was reported.
async fn collect(rx: &mut ChangeReceiver, want: &[&str]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    while !want.iter().all(|t| seen.contains(*t)) {
        seen.extend(next(rx).await);
    }
    seen.into_iter().collect()
}

async fn assert_quiet(rx: &mut ChangeReceiver) {
    let got = tokio::time::timeout(Duration::from_millis(750), rx.recv()).await;
    assert!(got.is_err(), "unexpected notification: {got:?}");
}

#[tokio::test]
async fn writes_notify_only_their_tenant_debounced() {
    let a = postgres_store().await;
    let b = postgres_store().await;
    let feed = ChangeFeed::connect(&runtime_url()).await.unwrap();
    let mut a_rx = feed.subscribe(a.user_id());
    let mut a_rx2 = feed.subscribe(a.user_id());
    let mut b_rx = feed.subscribe(b.user_id());

    a.set_setting("theme", "dark").await.unwrap();
    a.upsert_entry(&EntryInput {
        id: None,
        kind: EntryKind::Experience,
        title: "Engineer".into(),
        org: "Acme".into(),
        location: String::new(),
        start_date: "2022-01".into(),
        end_date: None,
        description: String::new(),
        sort_hint: 0,
        archived: false,
        bullets: vec!["a".into(), "b".into()],
        skills: vec!["Rust".into()],
    })
    .await
    .unwrap();

    let want = ["settings", "entry", "bullet", "skill", "entry_skill"];
    collect(&mut a_rx, &want).await;
    collect(&mut a_rx2, &want).await;
    assert_quiet(&mut b_rx).await;

    b.delete_setting("missing").await.unwrap();
    assert_quiet(&mut b_rx).await;
    b.set_setting("theme", "light").await.unwrap();
    assert_eq!(next(&mut b_rx).await, ["settings"]);
    assert_quiet(&mut a_rx).await;
}

#[tokio::test]
async fn one_statement_across_tenants_notifies_each_tenant() {
    let a = postgres_store().await;
    let b = postgres_store().await;
    a.set_setting("shared", "1").await.unwrap();
    b.set_setting("shared", "1").await.unwrap();
    let feed = ChangeFeed::connect(&runtime_url()).await.unwrap();
    let mut a_rx = feed.subscribe(a.user_id());
    let mut b_rx = feed.subscribe(b.user_id());

    let admin = admin().await;
    sqlx::query("UPDATE settings SET value = '2' WHERE key = 'shared' AND user_id IN ($1, $2)")
        .bind(a.user_id().0)
        .bind(b.user_id().0)
        .execute(&admin)
        .await
        .unwrap();
    // Deleting the auth user cascades; the deletes notify too.
    sqlx::query("DELETE FROM auth.users WHERE id = $1")
        .bind(b.user_id().0)
        .execute(&admin)
        .await
        .unwrap();
    admin.close().await;

    assert_eq!(next(&mut a_rx).await, ["settings"]);
    collect(&mut b_rx, &["settings", "job_status", "job_view"]).await;
}

#[tokio::test]
async fn every_tenant_table_has_change_triggers() {
    postgres_store().await;
    let admin = admin().await;
    let tenant_tables: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname::text FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         JOIN pg_attribute a ON a.attrelid = c.oid AND a.attname = 'user_id'
         WHERE n.nspname = 'public' AND c.relkind = 'r' ORDER BY 1",
    )
    .fetch_all(&admin)
    .await
    .unwrap();
    let mut expected: Vec<String> = pg::CHANGE_TABLES.iter().map(|t| t.to_string()).collect();
    expected.sort();
    assert_eq!(tenant_tables, expected);
    for table in &tenant_tables {
        let triggers: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_trigger WHERE tgrelid = ('public.' || $1)::regclass
             AND tgfoid = 'public.notify_data_change'::regproc",
        )
        .bind(table)
        .fetch_one(&admin)
        .await
        .unwrap();
        assert_eq!(triggers, 3, "{table}");
    }
    admin.close().await;
}

#[tokio::test]
async fn dropped_receivers_unsubscribe() {
    let a = postgres_store().await;
    let feed = ChangeFeed::connect(&runtime_url()).await.unwrap();
    drop(feed.subscribe(a.user_id()));
    let mut live = feed.subscribe(a.user_id());
    a.set_setting("k", "v").await.unwrap();
    assert_eq!(next(&mut live).await, ["settings"]);
    assert_eq!(feed.subscriber_count(a.user_id()), 1);
}

#[tokio::test]
async fn lost_listener_connection_refreshes_every_table() {
    let a = postgres_store().await;
    let app = "cvd-change-feed-reconnect-test";
    let feed = ChangeFeed::connect(&format!("{}?application_name={app}", runtime_url()))
        .await
        .unwrap();
    let mut rx = feed.subscribe(a.user_id());
    let admin = admin().await;
    let killed: Vec<bool> = sqlx::query_scalar(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE application_name = $1",
    )
    .bind(app)
    .fetch_all(&admin)
    .await
    .unwrap();
    admin.close().await;
    assert_eq!(killed, [true]);
    collect(&mut rx, pg::CHANGE_TABLES).await;
    // The reconnected listener keeps delivering.
    a.set_setting("after", "reconnect").await.unwrap();
    assert_eq!(next(&mut rx).await, ["settings"]);
}
