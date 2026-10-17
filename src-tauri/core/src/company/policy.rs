//! When a cached company profile needs (re)research.
use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, Utc};

use crate::store::types::Company;

/// Profiles older than this are researched again on the next match.
pub const COMPANY_TTL: Duration = Duration::from_secs(30 * 24 * 3600);
const BACKOFF_BASE: Duration = Duration::from_secs(3600);
const BACKOFF_CAP: Duration = Duration::from_secs(7 * 24 * 3600);
/// SQLite's `CURRENT_TIMESTAMP` format (UTC).
const TS_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    Fresh,
    Research,
    /// A recent research attempt failed; wait before trying again.
    BackingOff,
}

pub fn parse_ts(s: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(s, TS_FORMAT).ok().map(|t| t.and_utc())
}

pub fn format_ts(t: DateTime<Utc>) -> String {
    t.format(TS_FORMAT).to_string()
}

/// A profile researched within the TTL is fresh whatever its status (a failed
/// forced refresh keeps the old profile); otherwise a pending backoff wins.
pub fn need(c: &Company, now: DateTime<Utc>) -> Need {
    let ttl = chrono::Duration::from_std(COMPANY_TTL).expect("TTL fits");
    if c.researched_at.as_deref().and_then(parse_ts).is_some_and(|at| at + ttl > now) {
        return Need::Fresh;
    }
    if c.retry_after.as_deref().and_then(parse_ts).is_some_and(|at| at > now) {
        return Need::BackingOff;
    }
    Need::Research
}

/// Wait after the `fail_count`-th consecutive failure: 1 h doubling, capped at 7 days.
pub fn backoff(fail_count: i64) -> Duration {
    let exp = fail_count.clamp(1, 16) as u32 - 1;
    (BACKOFF_BASE * 2u32.pow(exp)).min(BACKOFF_CAP)
}
