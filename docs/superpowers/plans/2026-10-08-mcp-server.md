# CV Store MCP Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A stdio MCP server binary (`cvd-mcp`) that lets local agents (Claude Code, OpenCode) read and insert profile entries (experience, projects, …) into the app's SQLite DB, so they can migrate Notion content into the app.

**Architecture:** A second binary in `src-tauri` that reuses `cv_decisions_lib::store::Store` directly (no Tauri runtime). Tool handlers live in a library module `mcp` so they're unit-testable against an in-memory store; the binary only resolves the DB path and serves over stdio. Validation sits in front of `Store::upsert_entry`.

**Tech Stack:** Rust 2021, `rmcp` (official MCP Rust SDK, features `server`, `transport-io`, `macros`), `schemars`, `sqlx` sqlite (existing), `tokio`.

**Spec:** No separate spec. This plan follows the design discussion in the session that wrote it: expose list/upsert tools over the existing `Store`, share the DB with the app, validate agent input, and keep deletion out.

## Global Constraints

- The DB path must match the app's: `<app_data_dir>/cv.db`, where the identifier is `com.diego.cv-decisions` (macOS: `~/Library/Application Support/com.diego.cv-decisions/cv.db`). `CVD_DB_PATH` env var overrides it.
- No `delete_entry` tool. Updates require an explicit `id`.
- Dates are `YYYY-MM` strings (same as the existing `start_date` data, e.g. `"2022-01"`). `end_date` is `None` for current roles.
- stdout is reserved for the MCP protocol. All logging goes to stderr.
- Pin `rmcp` to the latest 0.x release and pin `schemars` to the major version that `rmcp` re-exports. Run `cargo tree -i schemars` to confirm.

## Review Focus

1. **App and server open the DB at the same time:** writes from the MCP server while the app is open must not fail with `database is locked`. Fix: enable WAL and a 5 s busy_timeout in `Store::open` (Task 1).
2. **The agent re-inserts an entry it already created:** duplicates are the most likely data-quality bug. Fix: `upsert_entry` rejects an insert whose (kind, title, org, start_date) matches an existing entry, returns that entry's id, and points the agent to the `id` update path (Task 2).
3. **Skill-name drift:** the agent writes `"rust"` when `"Rust"` exists. The skill table is NOCASE, so these already dedupe. Whitespace variants (`" Rust "`) do not, so trim every skill name (Task 2).
4. **Malformed dates such as `"Jan 2022"` or `"2022"`:** the tool returns an error that tells the agent the expected format. Nothing is written (Task 2).
5. **The server starts before the app has ever run:** `Store::open` would create a fresh DB and run migrations. That is acceptable, but the parent directory must be created first, or startup fails (Task 3).

---

## File Structure

- Modify `src-tauri/Cargo.toml`: add `rmcp`, `schemars`, and the `[[bin]] cvd-mcp` target.
- Modify `src-tauri/src/lib.rs`: change `mod store` to `pub mod store` and add `pub mod mcp`.
- Modify `src-tauri/src/store/mod.rs`: add WAL and busy_timeout to `open`, and add `find_entry_by_key`.
- Modify `src-tauri/src/store/types.rs`: add `#[derive(schemars::JsonSchema)]` to `EntryInput`, `EntryKind`, `Profile`, `Entry`, `Bullet`, `Skill`.
- Create `src-tauri/src/mcp/mod.rs`: `CvServer` with tool handlers.
- Create `src-tauri/src/mcp/validate.rs`: pure input validation.
- Create `src-tauri/src/mcp/tests.rs`.
- Create `src-tauri/src/bin/cvd-mcp.rs`: path resolution and stdio serve.
- Modify `README.md`: add a short "Agent access (MCP)" section with the registration commands.

---

### Task 1: Store concurrency and lookup

**Files:**
- Modify: `src-tauri/src/store/mod.rs:134-145`
- Test: `src-tauri/src/store/tests.rs`

**Interfaces:**
- Produces: `Store::find_entry_by_key(&self, kind: EntryKind, title: &str, org: &str, start_date: &str) -> Result<Option<i64>>`. It matches case-insensitively on title and org.

- [ ] **Step 1: Write failing tests**

```rust
#[tokio::test]
async fn find_entry_by_key_matches_case_insensitively() {
    let s = store().await;
    let id = s.upsert_entry(&input("Engineer", &[], &[])).await.unwrap();
    assert_eq!(s.find_entry_by_key(EntryKind::Experience, "engineer", "ACME", "2022-01").await.unwrap(), Some(id));
    assert_eq!(s.find_entry_by_key(EntryKind::Project, "Engineer", "Acme", "2022-01").await.unwrap(), None);
}

#[tokio::test]
async fn file_db_uses_wal() {
    let dir = std::env::temp_dir().join(format!("cvd-wal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let s = Store::open(&format!("sqlite://{}", dir.join("t.db").display())).await.unwrap();
    let mode: String = sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&s.pool).await.unwrap();
    assert_eq!(mode, "wal");
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run: `cargo test --lib store::tests::find_entry_by_key store::tests::file_db_uses_wal`
Expected: compile error (no `find_entry_by_key`). Then `assert` fails on `"delete"`.

- [ ] **Step 3: Implement**

In `open`, chain `.journal_mode(SqliteJournalMode::Wal).busy_timeout(Duration::from_secs(5))` for non-memory URLs. Add `find_entry_by_key`, using `title = ? COLLATE NOCASE AND org = ? COLLATE NOCASE`.

- [ ] **Step 4: Run `cargo test --lib store` and verify it passes**

- [ ] **Step 5: Commit**: `feat(store): WAL + busy timeout, find_entry_by_key`

---

### Task 2: Validation and MCP tool handlers

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/src/store/types.rs`
- Create: `src-tauri/src/mcp/mod.rs`, `src-tauri/src/mcp/validate.rs`, `src-tauri/src/mcp/tests.rs`

**Interfaces:**
- Consumes: `Store::{list_entries, list_skills, get_profile, upsert_profile, upsert_entry, find_entry_by_key}`.
- Produces:
  - `validate::normalize_entry(e: EntryInput) -> Result<EntryInput, String>`. It trims all strings and skill names, drops empty skills and bullets, and requires a non-empty `title` and `YYYY-MM` dates with `end >= start`.
  - `pub struct CvServer { store: Store }`, built with `CvServer::new(store: Store) -> Self`, and implementing `rmcp::ServerHandler`.
  - Tools (each one is also an `async fn` on `CvServer` so tests can call it directly), with JSON text results:
    - `list_entries { include_archived: bool }` returns `Vec<Entry>`.
    - `list_skills {}` returns `Vec<Skill>`.
    - `get_profile {}` returns `Option<Profile>`.
    - `upsert_profile { profile: Profile }` returns `"ok"`.
    - `upsert_entry { entry: EntryInput, dry_run: bool (default false) }` returns `{ "id": i64 | null, "entry": EntryInput }`.
  - Errors use `rmcp::ErrorData::invalid_params(msg, None)` for validation and duplicate failures, and `internal_error` for `StoreError`.

- [ ] **Step 1: Add deps and derives, and make `store` public**

Add to `Cargo.toml`: `rmcp = { version = "<latest 0.x>", features = ["server", "transport-io", "macros"] }` and `schemars = "<rmcp's major>"`. Add `features = [..., "io-std"]` to tokio. Derive `JsonSchema` on the types listed under File Structure. Then run `cargo check` and expect it to succeed.

- [ ] **Step 2: Write failing tests in `mcp/tests.rs`**

```rust
#[test] fn rejects_bad_date() { assert!(normalize_entry(entry("Jan 2022", None)).unwrap_err().contains("YYYY-MM")); }
#[test] fn rejects_end_before_start() { assert!(normalize_entry(entry("2023-01", Some("2022-01"))).is_err()); }
#[test] fn trims_and_drops_empty_skills() {
    let mut e = entry("2022-01", None); e.skills = vec![" Rust ".into(), "".into()];
    assert_eq!(normalize_entry(e).unwrap().skills, vec!["Rust"]);
}
#[tokio::test] async fn insert_then_duplicate_is_rejected_with_id() {
    let srv = server().await;
    let r = srv.upsert_entry(entry("2022-01", None), false).await.unwrap();
    let err = srv.upsert_entry(entry("2022-01", None), false).await.unwrap_err();
    assert!(err.message.contains(&format!("id {}", r.id.unwrap())));
}
#[tokio::test] async fn dry_run_writes_nothing() {
    let srv = server().await;
    assert_eq!(srv.upsert_entry(entry("2022-01", None), true).await.unwrap().id, None);
    assert!(srv.list_entries(true).await.unwrap().is_empty());
}
#[tokio::test] async fn update_by_id_replaces_bullets() { /* insert, then upsert with id + new bullets; list_entries shows new bullets only */ }
```

The helpers are `entry(start, end) -> EntryInput` (kind Project, title "CV app", org "Personal") and `server() -> CvServer` over `sqlite::memory:`.

- [ ] **Step 3: Run `cargo test --lib mcp` and verify it fails to compile**

- [ ] **Step 4: Implement `validate.rs` and `CvServer`**

Use `#[tool_router]` / `#[tool]` from rmcp, with one `Parameters<T>` struct per tool. `upsert_entry` works as follows: normalize; if `id` is None and `find_entry_by_key` hits, return an error containing `"already exists as id {id}; pass id to update"`; if `dry_run`, return without writing. Each tool description must tell the agent to call `list_entries` and `list_skills` before inserting. `get_info()` returns server name `cvd-mcp` and instructions that describe the Notion migration workflow and the date format.

- [ ] **Step 5: Run `cargo test --lib` and verify all tests pass**

- [ ] **Step 6: Commit**: `feat(mcp): CvServer tools with validation and dedupe`

---

### Task 3: `cvd-mcp` binary and registration docs

**Files:**
- Modify: `src-tauri/Cargo.toml` (add `[[bin]] name = "cvd-mcp" path = "src/bin/cvd-mcp.rs"`), `README.md`
- Create: `src-tauri/src/bin/cvd-mcp.rs`

**Interfaces:**
- Consumes: `CvServer::new`, `Store::open`.
- Produces: `fn db_path() -> PathBuf`. It returns `CVD_DB_PATH` when set; otherwise `dirs`-style app data: `$HOME/Library/Application Support/com.diego.cv-decisions/cv.db` on macOS, `%APPDATA%\com.diego.cv-decisions\cv.db` on Windows, `$XDG_DATA_HOME|~/.local/share/com.diego.cv-decisions/cv.db` on Linux. Add the `dirs` crate and use `dirs::data_dir()`, which matches Tauri's `app_data_dir`.

- [ ] **Step 1: Implement `main`**

Use `#[tokio::main]`. Run `create_dir_all(parent)`, then `Store::open`, then `CvServer::new(store).serve(rmcp::transport::stdio()).await?.waiting().await`. Errors go to stderr.

- [ ] **Step 2: Build with `cargo build --bin cvd-mcp` and expect it to succeed**

- [ ] **Step 3: Smoke-test with the MCP Inspector**

Run: `CVD_DB_PATH=$TMPDIR/cvd-smoke.db npx @modelcontextprotocol/inspector target/debug/cvd-mcp`
Expected: 5 tools are listed. `upsert_entry` with a valid project returns an id, and `list_entries` shows it.

- [ ] **Step 4: Test against the real app**

With the app open, insert an entry through the Inspector. Reopen the Entries screen and confirm the entry appears with no lock error.

- [ ] **Step 5: Add a README section**

```bash
claude mcp add cvd -- /abs/path/src-tauri/target/release/cvd-mcp
```

Add the OpenCode `opencode.json` snippet (`"mcp": { "cvd": { "type": "local", "command": ["/abs/path/cvd-mcp"] } }`) and one example prompt: "Read my Notion page X and add each project via cvd; check list_entries first."

- [ ] **Step 6: Commit**: `feat(mcp): cvd-mcp stdio binary + setup docs`

---

## Out of scope (YAGNI)

- Deleting or archiving entries over MCP. The user does this in the UI.
- Live UI refresh when the server writes. Re-querying on navigation is enough for now.
- HTTP/SSE transport and authentication. Access is local and over stdio only.
