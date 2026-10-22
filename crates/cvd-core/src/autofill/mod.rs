//! Auto-apply: the profile facts cvd may put into an application form.
//! See docs/superpowers/specs/2026-10-17-auto-apply-design.md.

pub mod defaults;
pub mod facts;
pub mod field;
pub mod matcher;
pub mod picker;

#[cfg(test)]
mod matcher_tests;
#[cfg(test)]
mod picker_tests;
#[cfg(test)]
mod tests;

pub use defaults::{ApplyDefaults, APPLY_DEFAULTS_KEY};
pub use facts::{build_facts, Fact};

#[cfg(test)]
mod eval_tests;
pub mod session;
#[cfg(test)]
mod window_tests;
