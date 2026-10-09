//! Company context: keys, freshness policy, rendering and the research agent.
mod key;

pub use key::{domain_from_url, normalize_name};

#[cfg(test)]
mod tests;
