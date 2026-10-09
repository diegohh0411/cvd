//! Company context: keys, freshness policy, rendering and the research agent.
mod key;
mod policy;

pub use key::{domain_from_url, normalize_name};
pub use policy::{backoff, need, parse_ts, format_ts, Need, COMPANY_TTL};

#[cfg(test)]
mod tests;
