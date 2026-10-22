# CVD

A local-first desktop app that tailors your CV to each job. Keep one library of your real experience, paste a job description, and CVD ranks your entries and bullets against the role, then exports a one-page, ATS-friendly PDF.

- **Truthful:** it only selects and orders your own content. It never writes or rewrites it.
- **Private:** your data lives in a local SQLite database. Matching runs on a local model by default; connect to a server you run on localhost, or add a key for optional online providers (OpenRouter, SubQ, TypeSafe). Company research also sends search queries to Exa.
- **Explainable:** you can see why each item was picked, override any choice, and watch the page fill live.

## How it works

1. Build your library: profile, experience, projects, education, volunteering and skills.
2. Paste a job description. A decision model scores each entry, bullet and skill against it.
3. Review the pick, check or uncheck items, and reorder them. The page fit updates as you edit.
4. Export a text-extractable PDF (rendered with Typst) that stays within your page budget.

## Stack

Tauri 2 (Rust core), Angular 22 with ZardUI, SQLite, user-run llama.cpp (or a compatible HTTP server) for local inference, Typst for PDFs. Design notes live in [`docs/superpowers`](docs/superpowers).

## Development

Prerequisites: [Bun](https://bun.sh), [Rust](https://rustup.rs) and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
bun install
bun run tauri dev                              # run the desktop app
bun run test                                   # Angular tests
cargo test --workspace
```

The Cargo workspace contains shared domain logic in `crates/cvd-core`, the stdio server in `crates/cvd-mcp`, and the Tauri shell in `apps/desktop/src-tauri`. Templates, fonts, and migrations are embedded in core.

### Local inference

CVD does not install models or start an inference process. Install [llama.cpp](https://github.com/ggml-org/llama.cpp#quick-start) yourself (release binaries or a source build), download a compatible GGUF, and start a server:

```bash
llama-server -m /path/to/decision.gguf --host 127.0.0.1 --port 8080 -b 16384 -ub 16384
```

In **AI Models → Connections → Local server**, save `http://127.0.0.1:8080/v1` and an optional API key. CVD discovers models from `/v1/models`; choose them in the Fast Model and Smart Model pickers, then use the connection tests to verify their capabilities. Connections are limited to loopback addresses.

**Fast Model (System One):** needs a compatible decision model (e.g. Clef, Kev or d1) and a server implementing `POST /v1/systemone`, including the model's logprobs/grammar support. An OpenAI-compatible chat endpoint alone is insufficient. The `-b 16384 -ub 16384` flags preserve the previous bundled server's batch settings, needed for large decision prompts. Server builds without System One support will fail the Fast Model test with a capability error.

**Smart Model (company research):** needs OpenAI-compatible `/v1/chat/completions` with tool calling and an Exa key. For llama.cpp, use a tool-capable chat GGUF and add `--jinja -c 32768`. To use different local models simultaneously, expose them through a single compatible multi-model endpoint (such as [llama.cpp router mode](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md#using-multiple-models)); the app sends the selected model ID with every request. CVD never stops your server when you switch models or exit.

On upgrade, old registry-based local selections are cleared and a dismissible notice points to Local server setup. Online selections and match history are preserved. Old GGUFs are kept; you can remove the app data `models/` folder yourself (on macOS: `~/Library/Application Support/com.diego.cvd/models/`).

## Agent access (MCP)

`cvd-mcp` is a stdio MCP server that lets local agents read and add CV entries in the app's database (`~/Library/Application Support/com.diego.cvd/cv.db` on macOS; override with `CVD_DB_PATH`). It's safe to run while the app is open, but reopen a screen to see new entries.

Tools: `list_entries`, `list_skills`, `get_profile`, `upsert_profile`, `upsert_entry` (supports `dry_run`), plus application tracking: `list_jobs`, `list_job_statuses`, `upsert_job` (save a posting, or change a job's `status`/`url` by id) and `upsert_job_status` (add, rename or recolor a label). There is no delete tool.

The MCP server never migrates an existing database, so open the updated desktop app once before using the job tools.

```bash
cargo build --release -p cvd-mcp
```

Claude Code:

```bash
claude mcp add cvd -- "$PWD/target/release/cvd-mcp"
```

OpenCode (`opencode.json`):

```json
{ "mcp": { "cvd": { "type": "local", "command": ["/abs/path/target/release/cvd-mcp"] } } }
```

Example prompt: "Read my Notion page *Projects* and add each one via cvd. Check list_entries and list_skills first."

## License

[PolyForm Small Business 1.0.0](LICENSE): free for individuals, noncommercial use, and small businesses (fewer than 100 people and under US$1,000,000 revenue). Larger companies need a separate commercial license from the author.
