//! Company context: keys, freshness policy, rendering and the research agent.
pub mod agent;
pub mod coordinator;
mod key;
mod prompt;
mod tools;
mod policy;
mod render;

pub use key::{domain_from_url, is_job_board, normalize_name};
pub use render::render;
pub use policy::{backoff, format_ts, need, Need};
#[cfg(test)]
pub use policy::parse_ts;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod agent_tests;
#[cfg(test)]
mod coordinator_tests;
