use super::{tests::postgres_store, *};

#[tokio::test]
async fn tenants_cannot_read_mutate_or_link_each_others_rows() {
    let a = postgres_store().await;
    let database = a.store.clone();
    let b = postgres_store().await;
    // Exercise both tenants through the SAME pool, including after failed writes.
    let b = database.for_user(b.user_id());
    a.set_setting("same", "a").await.unwrap();
    b.set_setting("same", "b").await.unwrap();
    a.set_secret("same", "a-secret").await.unwrap();
    b.set_secret("same", "b-secret").await.unwrap();
    a.remember_label("same", "a-fact").await.unwrap();
    b.remember_label("same", "b-fact").await.unwrap();
    let aj = a.create_jd("A", "same", "a-text", "", None).await.unwrap();
    let bj = b.create_jd("B", "same", "b-text", "", None).await.unwrap();
    let ac = a
        .create_company("same", "same", Some("same.example"))
        .await
        .unwrap();
    let bc = b
        .create_company("same", "same", Some("same.example"))
        .await
        .unwrap();
    a.add_alias("alias", ac.id).await.unwrap();
    b.add_alias("alias", bc.id).await.unwrap();
    assert_eq!(
        a.find_company(None, "alias").await.unwrap().unwrap().id,
        ac.id
    );
    assert_eq!(
        b.find_company(None, "alias").await.unwrap().unwrap().id,
        bc.id
    );
    assert!(a.get_company(bc.id).await.unwrap_err().is_not_found());
    assert!(a.get_jd(bj).await.unwrap().is_none());
    assert!(!a.update_jd_text(bj, "stolen").await.unwrap());
    assert!(a.create_run(bj, "model").await.is_err());
    assert!(a.set_jd_company(aj, Some(bc.id)).await.is_err());
    let bs = b.list_job_statuses().await.unwrap()[0].id;
    assert!(a.update_jd_tracking(aj, Some(bs), "").await.is_err());
    assert_eq!(b.get_jd(bj).await.unwrap().unwrap().raw_text, "b-text");
    assert_eq!(a.get_setting("same").await.unwrap().as_deref(), Some("a"));
    assert_eq!(b.get_setting("same").await.unwrap().as_deref(), Some("b"));
    a.delete_secret("same").await.unwrap();
    a.forget_label("same").await.unwrap();
    assert_eq!(
        b.get_secret("same").await.unwrap().as_deref(),
        Some("b-secret")
    );
    assert_eq!(b.apply_memory().await.unwrap()["same"], "b-fact");
    let ar = a.create_scan_run(ScanSource::Manual).await.unwrap();
    let br = b.create_scan_run(ScanSource::Manual).await.unwrap();
    let message = |run_id, subject: &str| NewProcessed {
        message_id: "same-message".into(),
        run_id,
        received_at: "2026-10-12 09:00:00".into(),
        subject: subject.into(),
        sender: "test@example.com".into(),
        matched_job_id: None,
        previous_status_id: None,
        suggested_status_id: None,
        confidence: None,
        summary: None,
        outcome: Outcome::NoChange,
        job_related: None,
        body: None,
    };
    a.record_processed(&message(ar, "A"), None).await.unwrap();
    b.record_processed(&message(br, "B"), None).await.unwrap();
    a.mark_all_read().await.unwrap();
    assert_eq!(b.unread_count().await.unwrap(), 1);
    assert_eq!(
        a.get_processed("same-message")
            .await
            .unwrap()
            .unwrap()
            .subject,
        "A"
    );
    assert!(a
        .record_processed(&message(br, "wrong run"), None)
        .await
        .is_err());
    assert_eq!(
        a.get_processed("same-message")
            .await
            .unwrap()
            .unwrap()
            .subject,
        "A"
    );
    assert_eq!(a.list_jds().await.unwrap().len(), 1);
    assert_eq!(b.list_jds().await.unwrap().len(), 1);
}

#[tokio::test]
async fn rls_blocks_unfiltered_reads_and_cross_tenant_writes_and_context_resets() {
    let a = postgres_store().await;
    let b = postgres_store().await;
    a.set_setting("private", "a").await.unwrap();
    b.set_setting("private", "b").await.unwrap();
    let url = std::env::var("CVD_TEST_POSTGRES_URL").unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(a.user_id().0.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    // Deliberately forget the user_id filter: RLS is the backstop.
    let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as("SELECT user_id, value FROM settings")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(rows, vec![(a.user_id().0, "a".into())]);
    let changed = sqlx::query("UPDATE settings SET value = 'stolen' WHERE user_id = $1")
        .bind(b.user_id().0)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(changed.rows_affected(), 0);
    tx.commit().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM settings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 0,
        "tenant context must not survive commit/pool reuse"
    );
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(a.user_id().0.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let error =
        sqlx::query("INSERT INTO settings(user_id, key, value) VALUES ($1, 'injected', 'x')")
            .bind(b.user_id().0)
            .execute(&mut *tx)
            .await
            .unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("42501")
    );
    tx.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM settings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "tenant context must not survive rollback");
    assert_eq!(
        b.get_setting("private").await.unwrap().as_deref(),
        Some("b")
    );
    pool.close().await;
}

#[tokio::test]
async fn all_tenant_tables_force_rls_and_unsafe_runtime_roles_are_rejected() {
    let _ = postgres_store().await;
    let admin_url = std::env::var("CVD_TEST_POSTGRES_ADMIN_URL").unwrap();
    assert!(matches!(
        Store::open_postgres(&admin_url).await,
        Err(StoreError::Invalid(_))
    ));
    let pool = sqlx::PgPool::connect(&admin_url).await.unwrap();
    let unprotected: Vec<String> = sqlx::query_scalar(
        "SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE n.nspname = 'public' AND c.relkind = 'r' AND c.relname != '_sqlx_migrations'
         AND (NOT c.relrowsecurity OR NOT c.relforcerowsecurity)",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(unprotected.is_empty(), "{unprotected:?}");
    pool.close().await;
}
