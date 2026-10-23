//! Dev server: one fixed user until auth lands in #67.
//!
//! - `DATABASE_URL`: runtime role (no superuser/BYPASSRLS), direct or session-mode connection
//! - `CVD_MIGRATE_URL`: optional schema-owner URL; migrations run on startup when set
//! - `CVD_DEV_USER_ID`: an existing `auth.users` id (default: the nil UUID)
//! - `CVD_STATIC_DIR`: web build to serve (default `apps/web/dist/cvd/browser`)
//! - `CVD_PORT`: port on 127.0.0.1 (default 8787)
use std::{net::SocketAddr, path::PathBuf};

use cvd_core::{
    core,
    store::{ChangeFeed, Store, UserId},
};
use cvd_server::{router, Events};

type Error = Box<dyn std::error::Error + Send + Sync>;

fn env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let url = env("DATABASE_URL").ok_or("DATABASE_URL is required")?;
    if let Some(owner) = env("CVD_MIGRATE_URL") {
        Store::migrate_postgres(&owner).await?;
    }
    let user = UserId(match env("CVD_DEV_USER_ID") {
        Some(id) => id.parse()?,
        None => uuid::Uuid::nil(),
    });
    let store = Store::open_postgres(&url).await?;
    let tenant = store.for_user(user);
    // Dev stand-in for sign-up provisioning. Users can delete every status later, so only a
    // user with neither statuses nor views is fresh; defaults are inserted all-or-nothing.
    if tenant.list_job_statuses().await?.is_empty() && tenant.list_job_views().await?.is_empty() {
        store.create_user_defaults(user).await?;
    }

    let events = Events::default();
    let state = core::start(tenant, events.emitter()).await?;
    let feed = ChangeFeed::connect(&url).await?;
    core::forward_changes(feed.subscribe(user), events.emitter());

    let static_dir = env("CVD_STATIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("apps/web/dist/cvd/browser"));
    if !static_dir.join("index.html").is_file() {
        tracing::warn!(dir = %static_dir.display(), "no web build found; run `bun run build:web`");
    }
    let app = router(state, events, Some(&static_dir))
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let port = env("CVD_PORT")
        .map(|p| p.parse())
        .transpose()?
        .unwrap_or(8787);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(user = %user.0, "listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    drop(feed);
    Ok(())
}
