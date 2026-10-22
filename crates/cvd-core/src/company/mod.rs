//! Company context: keys, freshness policy, rendering and the research agent.
pub mod agent;
pub mod coordinator;
mod key;
mod policy;
mod prompt;
mod render;
mod tools;

pub use key::{domain_from_url, is_job_board, normalize_name};
#[cfg(test)]
pub use policy::parse_ts;
pub use policy::{backoff, format_ts, need, Need};
pub use render::render;

#[cfg(test)]
mod agent_tests;
#[cfg(test)]
mod coordinator_tests;
#[cfg(test)]
mod tests;
