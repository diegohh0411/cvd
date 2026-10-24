//! One set of running services per signed-in user, started on their first request and
//! unloaded once idle.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard, Weak},
    time::Duration,
};

use cvd_core::{
    core,
    services::AppError,
    state::AppState,
    store::{ChangeFeed, Store, UserId},
};
use futures_util::{future::BoxFuture, TryFutureExt};
use tokio::{sync::OnceCell, task::AbortHandle, time::Instant};

use crate::{limits::RateLimits, Events};

/// A user's services, the event stream their UI listens to, and their rate limits.
pub struct Tenant {
    pub state: Arc<AppState>,
    pub events: Events,
    /// Shared with `Tenants`, which keeps them after the user unloads.
    pub limits: Arc<RateLimits>,
    /// Forwards this user's database changes to `events`.
    forward: Option<AbortHandle>,
}

impl Tenant {
    pub fn new(state: Arc<AppState>, events: Events) -> Self {
        Self {
            state,
            events,
            limits: Arc::default(),
            forward: None,
        }
    }

    /// Seeds a new user's defaults, then starts their services.
    pub async fn start(
        store: &Store,
        feed: Option<&ChangeFeed>,
        user: UserId,
    ) -> Result<Self, AppError> {
        provision(store, user).await?;
        let events = Events::default();
        let state = core::start(store.for_user(user), events.emitter()).await?;
        let forward = feed.map(|feed| {
            core::forward_changes(feed.subscribe(user), events.emitter()).abort_handle()
        });
        let mut tenant = Self::new(state, events);
        tenant.forward = forward;
        Ok(tenant)
    }

    /// Nothing would notice it unloading: no open event stream and nothing running.
    fn idle(&self) -> bool {
        self.events.listeners() == 0 && !self.state.busy()
    }

    /// Like `idle`, but also stops new background scans, so check it last.
    fn retire(&self) -> bool {
        self.events.listeners() == 0 && self.state.try_retire()
    }
}

/// Background tasks hold the state, so stop them or it never frees.
impl Drop for Tenant {
    fn drop(&mut self) {
        self.state.watcher.shutdown();
        if let Some(forward) = &self.forward {
            forward.abort();
        }
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

/// How many users stay loaded, and for how long.
#[derive(Clone, Copy, Debug)]
pub struct Residency {
    /// Most users loaded at once; past it, the least recently used idle user unloads.
    pub max_users: usize,
    /// An idle user unloads this long after their last request.
    pub idle_after: Duration,
}

impl Default for Residency {
    fn default() -> Self {
        Self {
            max_users: 500,
            idle_after: Duration::from_secs(15 * 60),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TenantError {
    #[error("The server is at capacity; try again in a minute.")]
    Full,
    #[error(transparent)]
    Start(#[from] AppError),
}

/// Starts one user's services.
pub type Starter =
    Arc<dyn Fn(UserId) -> BoxFuture<'static, Result<Tenant, AppError>> + Send + Sync>;

struct Slot {
    tenant: Arc<OnceCell<Arc<Tenant>>>,
    last_used: Instant,
}

impl Slot {
    /// Nobody is starting or using it and it is idle (or failed to start).
    fn evictable(&self) -> bool {
        Arc::strong_count(&self.tenant) == 1
            && self
                .tenant
                .get()
                .is_none_or(|t| Arc::strong_count(t) == 1 && t.idle())
    }

    /// `evictable`, and its watcher can no longer start a scan. Call with the slots lock
    /// held and remove the slot when true, since nothing else will un-retire it.
    fn retire(&self) -> bool {
        self.evictable() && self.tenant.get().is_none_or(|t| t.retire())
    }
}

pub struct Tenants {
    start: Starter,
    residency: Residency,
    slots: Mutex<HashMap<UserId, Slot>>,
    /// Outlives the user's slot, so unloading doesn't refill their buckets.
    limits: Mutex<HashMap<UserId, Arc<RateLimits>>>,
}

impl Tenants {
    /// `feed` forwards writes from other connections (MCP, other instances) to each user.
    pub fn new(store: Store, feed: Option<ChangeFeed>, residency: Residency) -> Self {
        let feed = feed.map(Arc::new);
        Self::with_starter(
            Arc::new(move |user| {
                let (store, feed) = (store.clone(), feed.clone());
                Box::pin(async move { Tenant::start(&store, feed.as_deref(), user).await })
            }),
            residency,
        )
    }

    pub fn with_starter(start: Starter, residency: Residency) -> Self {
        Self {
            start,
            residency,
            slots: Mutex::default(),
            limits: Mutex::default(),
        }
    }

    fn slots(&self) -> MutexGuard<'_, HashMap<UserId, Slot>> {
        self.slots.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Concurrent first requests for one user share a single start.
    pub async fn get(&self, user: UserId) -> Result<Arc<Tenant>, TenantError> {
        let mut unloaded = None;
        let cell = {
            let mut slots = self.slots();
            let now = Instant::now();
            if !slots.contains_key(&user) && slots.len() >= self.residency.max_users {
                unloaded = Self::evict_lru(&mut slots);
                if unloaded.is_none() {
                    return Err(TenantError::Full);
                }
            }
            let slot = slots.entry(user).or_insert_with(|| Slot {
                tenant: Arc::default(),
                last_used: now,
            });
            slot.last_used = now;
            slot.tenant.clone()
        };
        // Stopping its tasks happens outside the lock.
        drop(unloaded);
        let tenant = cell
            .get_or_try_init(|| {
                (self.start)(user).map_ok(|mut t| {
                    t.limits = self.limits_for(user);
                    Arc::new(t)
                })
            })
            .await?;
        Ok(tenant.clone())
    }

    fn limits_for(&self, user: UserId) -> Arc<RateLimits> {
        let mut limits = self.limits.lock().unwrap_or_else(|e| e.into_inner());
        limits.entry(user).or_default().clone()
    }

    fn evict_lru(slots: &mut HashMap<UserId, Slot>) -> Option<Slot> {
        let mut candidates: Vec<_> = slots
            .iter()
            .filter(|(_, s)| s.evictable())
            .map(|(u, s)| (s.last_used, *u))
            .collect();
        candidates.sort_by_key(|(at, _)| *at);
        let user = candidates
            .into_iter()
            .map(|(_, u)| u)
            .find(|u| slots[u].retire())?;
        slots.remove(&user)
    }

    /// Unloads users idle for `idle_after`; returns how many.
    pub fn sweep(&self) -> usize {
        let cutoff = self.residency.idle_after;
        let unloaded: Vec<Slot> = {
            let mut slots = self.slots();
            let now = Instant::now();
            let stale: Vec<UserId> = slots
                .iter()
                .filter(|(_, s)| now.duration_since(s.last_used) >= cutoff && s.retire())
                .map(|(u, _)| *u)
                .collect();
            stale.iter().filter_map(|u| slots.remove(u)).collect()
        };
        // Unloaded users' buckets go once they've refilled; loaded users share theirs.
        self.limits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, l| Arc::strong_count(l) > 1 || !l.is_full());
        unloaded.len()
    }

    /// Users currently loaded.
    pub fn resident(&self) -> usize {
        self.slots().len()
    }

    /// Sweeps every `every` until the `Tenants` is dropped.
    pub fn spawn_sweeper(self: &Arc<Self>, every: Duration) -> tokio::task::JoinHandle<()> {
        let tenants: Weak<Self> = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(every);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let Some(tenants) = tenants.upgrade() else {
                    return;
                };
                let n = tenants.sweep();
                if n > 0 {
                    tracing::debug!(
                        unloaded = n,
                        resident = tenants.resident(),
                        "swept idle users"
                    );
                }
            }
        })
    }
}
