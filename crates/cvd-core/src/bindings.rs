//! TypeScript mirrors of the types the webview sees, rendered with ts-rs into
//! `apps/web/src/app/core/ipc/bindings.ts`. The `bindings_are_fresh` test fails
//! when that file is stale; `CVD_UPDATE_BINDINGS=1 cargo test -p cvd-core bindings`
//! rewrites it.
use ts_rs::{Config, TS};

use crate::{
    autofill::{ApplyDefaults, Fact},
    company::agent::ResearchStep,
    mail::watch::{WatchState, WatcherStatus},
    matcher::{Ranked, RankedEntry},
    providers::{ProviderModels, RemoteModel},
    render::fit::{Override, SelectedEntry, Selection, Target},
    services::{
        AppError, ConnectionView, FitOutput, KeyStatus, LocalConnectionView, MailAccountView,
        ProcessedEmailView, ScanState,
    },
    state::EngineStatus,
    store::{types::*, InboxFilter, Outcome, ProcessedEmail, ScanSource},
};

pub const PATH: &str = "apps/web/src/app/core/ipc/bindings.ts";

const HEADER: &str = "\
// Generated from the Rust types by crates/cvd-core/src/bindings.rs; do not edit.
// Regenerate: CVD_UPDATE_BINDINGS=1 cargo test -p cvd-core bindings
import type { AppErrorKind, EngineState, JobFilter, JobSort, ProviderId } from './types';
";

macro_rules! decls {
    ($cfg:ident; $($t:ty),* $(,)?) => {
        vec![$(<$t as TS>::decl(&$cfg)),*]
    };
}

/// The whole `bindings.ts` file.
pub fn render() -> String {
    let cfg = Config::new().with_large_int("number");
    let decls = decls![cfg;
        Profile, ApplyDefaults, Fact, EntryKind, Bullet, Skill, Entry, EntryInput,
        JobDescription, ViewLayout, JobView, JobViewInput, StatusColor, JobStatus,
        RunStatus, MatchRun, TargetType, ScoreRow, RankedEntry, Ranked, SelectedEntry,
        Selection, Target, Override, FitOutput, Cv, EngineStatus, AppError, KeyStatus,
        RemoteModel, ConnectionView, ProviderModels, LocalConnectionView, Source,
        CompanyProfile, CompanyStatus, Company, ResearchStep, MailAccountView, Outcome,
        ProcessedEmail, ProcessedEmailView, InboxFilter, WatchState, WatcherStatus,
        ScanSource, ScanState,
    ];
    let mut out = HEADER.to_string();
    for d in decls {
        out.push('\n');
        out.push_str("export ");
        out.push_str(&d);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn bindings_are_fresh() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(super::PATH);
        let want = super::render();
        if std::env::var_os("CVD_UPDATE_BINDINGS").is_some() {
            std::fs::write(&path, &want).unwrap();
        }
        let have = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            have == want,
            "{} is stale; run CVD_UPDATE_BINDINGS=1 cargo test -p cvd-core bindings",
            super::PATH
        );
    }
}
