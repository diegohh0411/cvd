//! Web server. With `SUPABASE_URL` and `SUPABASE_ANON_KEY` set, every request needs a Supabase
//! access token and each user gets their own services; without them it runs as one dev user.
//!
//! - `DATABASE_URL`: runtime role (no superuser/BYPASSRLS), direct or session-mode connection
//! - `CVD_MIGRATE_URL`: optional schema-owner URL; migrations run on startup when set
//! - `SUPABASE_URL`, `SUPABASE_ANON_KEY`: the project the web build signs in to
//! - `CVD_DEV_USER_ID`: without Supabase, an existing `auth.users` id (default: the nil UUID)
//! - `CVD_HOST`: address to bind (default 127.0.0.1)
//! - `CVD_STATIC_DIR`: web build to serve (default `apps/web/dist/cvd/browser`)
//! - `CVD_PORT`: port (default 8787)
use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use cvd_core::store::{ChangeFeed, Store, UserId};
use cvd_server::{
    auth::Verifier,
    router_with,
    tenants::{Tenant, Tenants},
    Access, ClientAuth,
};

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
    let store = Store::open_postgres(&url).await?;
    let feed = ChangeFeed::connect(&url).await?;
    let access = match (env("SUPABASE_URL"), env("SUPABASE_ANON_KEY")) {
        (Some(supabase_url), Some(supabase_anon_key)) => {
            tracing::info!(%supabase_url, "accounts enabled");
            Access::Accounts {
                verifier: Verifier::supabase(&supabase_url),
                tenants: Tenants::new(store, Some(feed)),
                client: ClientAuth {
                    supabase_url,
                    supabase_anon_key,
                },
            }
        }
        (None, None) => {
            let user = UserId(match env("CVD_DEV_USER_ID") {
                Some(id) => id.parse()?,
                None => uuid::Uuid::nil(),
            });
            tracing::warn!(user = %user.0, "no Supabase configured; every request is this user");
            Access::Open(Arc::new(Tenant::start(&store, Some(&feed), user).await?))
        }
        _ => return Err("set both SUPABASE_URL and SUPABASE_ANON_KEY, or neither".into()),
    };

    let static_dir = env("CVD_STATIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("apps/web/dist/cvd/browser"));
    if !static_dir.join("index.html").is_file() {
        tracing::warn!(dir = %static_dir.display(), "no web build found; run `bun run build:web`");
    }
    let app =
        router_with(access, Some(&static_dir)).layer(tower_http::trace::TraceLayer::new_for_http());

    let port = env("CVD_PORT")
        .map(|p| p.parse())
        .transpose()?
        .unwrap_or(8787);
    let host: std::net::IpAddr = env("CVD_HOST")
        .map(|h| h.parse())
        .transpose()?
        .unwrap_or([127, 0, 0, 1].into());
    let addr = SocketAddr::from((host, port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
