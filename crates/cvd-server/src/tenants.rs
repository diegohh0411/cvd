//! One set of running services per signed-in user, started on their first request.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use cvd_core::{
    core,
    services::AppError,
    state::AppState,
    store::{ChangeFeed, Store, UserId},
};
use tokio::sync::OnceCell;

use crate::Events;

/// A user's services and the event stream their UI listens to.
pub struct Tenant {
    pub state: Arc<AppState>,
    pub events: Events,
}

impl Tenant {
    /// Seeds a new user's defaults, then starts their services.
    pub async fn start(
        store: &Store,
        feed: Option<&ChangeFeed>,
        user: UserId,
    ) -> Result<Self, AppError> {
        provision(store, user).await?;
        let events = Events::default();
        let state = core::start(store.for_user(user), events.emitter()).await?;
        if let Some(feed) = feed {
            core::forward_changes(feed.subscribe(user), events.emitter());
        }
        Ok(Self { state, events })
    }
}

/// The desktop app seeds on first run; here the first sign-in does. Users can delete every
/// status later, so only a user with neither statuses nor views is fresh.
async fn provision(store: &Store, user: UserId) -> Result<(), AppError> {
    let tenant = store.for_user(user);
    let fresh = |t: &cvd_core::store::TenantStore| {
        let t = t.clone();
        async move {
            Ok::<_, AppError>(
                t.list_job_statuses().await?.is_empty() && t.list_job_views().await?.is_empty(),
            )
        }
    };
    if !fresh(&tenant).await? {
        return Ok(());
    }
    match store.create_user_defaults(user).await {
        Ok(()) => Ok(()),
        // Another server instance provisioned them first.
        Err(_) if !fresh(&tenant).await? => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub struct Tenants {
    store: Store,
    feed: Option<ChangeFeed>,
    running: Mutex<HashMap<UserId, Arc<OnceCell<Arc<Tenant>>>>>,
}

impl Tenants {
    /// `feed` forwards writes from other connections (MCP, other instances) to each user.
    pub fn new(store: Store, feed: Option<ChangeFeed>) -> Self {
        Self {
            store,
            feed,
            running: Mutex::default(),
        }
    }

    /// Concurrent first requests for one user share a single start.
    pub async fn get(&self, user: UserId) -> Result<Arc<Tenant>, AppError> {
        let cell = self
            .running
            .lock()
            .unwrap()
            .entry(user)
            .or_default()
            .clone();
        cell.get_or_try_init(|| async {
            Tenant::start(&self.store, self.feed.as_ref(), user)
                .await
                .map(Arc::new)
        })
        .await
        .cloned()
    }
}
