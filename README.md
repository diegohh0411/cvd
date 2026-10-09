# CVD

This template should help get you started developing with Tauri and Angular.

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) + [Angular Language Service](https://marketplace.visualstudio.com/items?itemName=Angular.ng-template).

## Agent access (MCP)

`cvd-mcp` is a stdio MCP server that lets local agents read and add CV entries in the app's database (`~/Library/Application Support/com.diego.cvd/cv.db` on macOS; override with `CVD_DB_PATH`). It's safe to run while the app is open, but reopen a screen to see new entries.

Tools: `list_entries`, `list_skills`, `get_profile`, `upsert_profile`, `upsert_entry` (supports `dry_run`), plus application tracking: `list_jobs`, `list_job_statuses`, `upsert_job` (save a posting, or change a job's `status`/`url` by id) and `upsert_job_status` (add, rename or recolor a label). There is no delete tool.

The MCP server never migrates an existing database, so open the updated desktop app once before using the job tools.

```bash
cargo build --release --manifest-path src-tauri/Cargo.toml --bin cvd-mcp
```

Claude Code:

```bash
claude mcp add cvd -- "$PWD/src-tauri/target/release/cvd-mcp"
```

OpenCode (`opencode.json`):

```json
{ "mcp": { "cvd": { "type": "local", "command": ["/abs/path/src-tauri/target/release/cvd-mcp"] } } }
```

Example prompt: "Read my Notion page *Projects* and add each one via cvd. Check list_entries and list_skills first."

## License

[PolyForm Small Business 1.0.0](LICENSE): free for individuals, noncommercial use, and small businesses (fewer than 100 people and under US$1,000,000 revenue). Larger companies need a separate commercial license from the author.
