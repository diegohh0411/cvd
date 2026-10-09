# CVD

A local-first desktop app that tailors your CV to each job. Keep one library of your real experience, paste a job description, and CVD ranks your entries and bullets against the role, then exports a one-page, ATS-friendly PDF.

- **Truthful:** it only selects and orders your own content. It never writes or rewrites it.
- **Private:** your data lives in a local SQLite database. Matching runs on a local model by default; the only network use is downloading the model, plus optional OpenRouter models if you add a key.
- **Explainable:** you can see why each item was picked, override any choice, and watch the page fill live.

## How it works

1. Build your library: profile, experience, projects, education, volunteering and skills.
2. Paste a job description. A decision model scores each entry, bullet and skill against it.
3. Review the pick, check or uncheck items, and reorder them. The page fit updates as you edit.
4. Export a text-extractable PDF (rendered with Typst) that stays within your page budget.

## Stack

Tauri 2 (Rust core), Angular 22 with ZardUI, SQLite, llama.cpp as a bundled `llama-server` sidecar, Typst for PDFs. Design notes live in [`docs/superpowers`](docs/superpowers).

## Development

Prerequisites: [Bun](https://bun.sh), [Rust](https://rustup.rs) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
bun install
bash src-tauri/scripts/fetch-llama-server.sh   # downloads the llama-server sidecar
bun run tauri dev                              # run the desktop app
bun run test                                   # Angular tests
cargo test --manifest-path src-tauri/Cargo.toml
```

Local inference is currently set up for macOS on Apple Silicon.

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
