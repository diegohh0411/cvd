//! Per-user rate limits on commands that spend AI credits, mail quota or CPU.
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// A token bucket: up to `burst` calls at once, then one more every `refill`.
#[derive(Clone, Copy, Debug)]
pub struct Limit {
    pub burst: u32,
    pub refill: Duration,
}

/// Commands that do another's work and so spend its tokens.
const SAME_AS: &[(&str, &str)] = &[("rescore", "start_match")];

/// The limited commands. Anything not listed is unlimited.
pub const LIMITS: &[(&str, Limit)] = &[
    (
        "start_match",
        Limit {
            burst: 10,
            refill: Duration::from_secs(15),
        },
    ),
    (
        "refresh_company",
        Limit {
            burst: 5,
            refill: Duration::from_secs(30),
        },
    ),
    (
        "start_scan",
        Limit {
            burst: 3,
            refill: Duration::from_secs(60),
        },
    ),
];

struct Bucket {
    tokens: f64,
    at: Instant,
}

/// One user's buckets.
pub struct RateLimits {
    limits: &'static [(&'static str, Limit)],
    buckets: Mutex<HashMap<&'static str, Bucket>>,
}

impl Default for RateLimits {
    fn default() -> Self {
        Self::new(LIMITS)
    }
}

impl RateLimits {
    pub fn new(limits: &'static [(&'static str, Limit)]) -> Self {
        Self {
            limits,
            buckets: Mutex::default(),
        }
    }

    /// Takes a token for `command`, or says how long until one is free.
    pub fn check(&self, command: &str) -> Result<(), Duration> {
        self.check_at(command, Instant::now())
    }

    fn check_at(&self, command: &str, now: Instant) -> Result<(), Duration> {
        let command = SAME_AS
            .iter()
            .find(|(c, _)| *c == command)
            .map_or(command, |(_, same)| same);
        let Some((name, limit)) = self.limits.iter().find(|(n, _)| *n == command) else {
            return Ok(());
        };
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        let bucket = buckets.entry(name).or_insert(Bucket {
            tokens: limit.burst as f64,
            at: now,
        });
        let refill = limit.refill.as_secs_f64();
        let earned = now.saturating_duration_since(bucket.at).as_secs_f64() / refill;
        bucket.tokens = (bucket.tokens + earned).min(limit.burst as f64);
        bucket.at = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            Err(Duration::from_secs_f64((1.0 - bucket.tokens) * refill))
        }
    }

    /// Every bucket has refilled, so forgetting them forgives nothing.
    pub fn is_full(&self) -> bool {
        self.is_full_at(Instant::now())
    }

    fn is_full_at(&self, now: Instant) -> bool {
        let buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        buckets.iter().all(|(name, b)| {
            self.limits
                .iter()
                .find(|(n, _)| n == name)
                .is_none_or(|(_, l)| {
                    let earned =
                        now.saturating_duration_since(b.at).as_secs_f64() / l.refill.as_secs_f64();
                    b.tokens + earned >= l.burst as f64
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST: &[(&str, Limit)] = &[(
        "start_match",
        Limit {
            burst: 2,
            refill: Duration::from_secs(10),
        },
    )];

    #[test]
    fn allows_a_burst_then_refills_one_token_per_period() {
        let limits = RateLimits::new(TEST);
        let t0 = Instant::now();
        assert!(limits.check_at("start_match", t0).is_ok());
        assert!(limits.check_at("start_match", t0).is_ok());
        let wait = limits.check_at("start_match", t0).unwrap_err();
        assert_eq!(wait, Duration::from_secs(10));
        let wait = limits
            .check_at("start_match", t0 + Duration::from_secs(4))
            .unwrap_err();
        assert_eq!(wait.as_secs(), 6);
        assert!(limits
            .check_at("start_match", t0 + Duration::from_secs(10))
            .is_ok());
        assert!(limits
            .check_at("start_match", t0 + Duration::from_secs(10))
            .is_err());
    }

    #[test]
    fn idle_time_never_banks_more_than_the_burst() {
        let limits = RateLimits::new(TEST);
        let later = Instant::now() + Duration::from_secs(3600);
        assert!(limits.check_at("start_match", later).is_ok());
        assert!(limits.check_at("start_match", later).is_ok());
        assert!(limits.check_at("start_match", later).is_err());
    }

    #[test]
    fn rescore_spends_start_match_tokens() {
        let limits = RateLimits::new(TEST);
        let t0 = Instant::now();
        assert!(limits.check_at("rescore", t0).is_ok());
        assert!(limits.check_at("start_match", t0).is_ok());
        assert!(limits.check_at("rescore", t0).is_err());
        assert!(limits.check_at("start_match", t0).is_err());
    }

    #[test]
    fn full_once_every_bucket_has_refilled() {
        let limits = RateLimits::new(TEST);
        let t0 = Instant::now();
        assert!(limits.is_full_at(t0));
        limits.check_at("start_match", t0).unwrap();
        assert!(!limits.is_full_at(t0 + Duration::from_secs(9)));
        assert!(limits.is_full_at(t0 + Duration::from_secs(10)));
    }

    #[test]
    fn unlisted_commands_are_unlimited() {
        let limits = RateLimits::new(TEST);
        for _ in 0..100 {
            assert!(limits.check("list_jobs").is_ok());
        }
    }
}
