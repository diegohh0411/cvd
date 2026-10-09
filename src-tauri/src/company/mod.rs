//! Company context: keys, freshness policy, rendering and the research agent.
mod key;
mod policy;
mod render;

pub use key::{domain_from_url, normalize_name};
pub use render::render;
pub use policy::{backoff, need, parse_ts, format_ts, Need};

#[cfg(test)]
mod tests;
