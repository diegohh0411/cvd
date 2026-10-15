//! CV Decisions business logic: storage, matching, rendering, research and mail,
//! with no dependency on Tauri. Front ends (the desktop app, the MCP server, a
//! future HTTP API) drive it through `commands` over a shared `state::AppState`.

pub mod commands;
mod company;
#[allow(dead_code)]
mod mail;
// Some items in these modules are only used by tests or by later plans.
#[allow(dead_code)]
pub mod engine;
#[allow(dead_code)]
mod matcher;
#[allow(dead_code)]
pub mod models;
mod providers;
#[allow(dead_code)]
mod render;
mod search;
mod web;
pub mod state;
#[allow(dead_code)]
pub mod store;
pub mod mcp;
