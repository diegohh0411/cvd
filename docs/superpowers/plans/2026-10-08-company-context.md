# Company Context Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every System One match is scored with a cached, web-researched company profile; a cache miss blocks the match while a local tool-calling agent researches the company.

**Architecture:** A `company` table (+ `company_alias`) caches typed profiles for 30 days. A `ResearchCoordinator` in `AppState` runs one `agentix` agent per company (single-flight, global concurrency cap) with three tools: `web_search` (a `SearchProvider`, Exa first), `fetch_page`, `submit_profile`. The agent talks to any OpenAI-compatible chat endpoint: a second llama.cpp sidecar (`--jinja`) serving a chat model, or OpenRouter. `start_match` awaits the profile and passes it into `jd_state`, so it becomes part of the System One `state`.

**Tech Stack:** Rust (Tauri 2, sqlx/SQLite, reqwest 0.13, tokio, `agentix` pinned, `html2text`), `wiremock` for tests, Angular + vitest frontend.

**Spec:** `docs/superpowers/specs/2026-10-08-company-context-design.md`

## Global Constraints

- TTL: 30 days from `researched_at`, a Rust constant `COMPANY_TTL`, never stored in SQL. Manual edits do not pause it.
- Agent budget: 25 tool rounds, 4 min wall clock (`MAX_ROUNDS = 25`, `WALL_CLOCK = 240 s`).
- Profile fields: `summary, industry, size, stage, headquarters, remote_policy, products, tech_stack[], sources[]`. No `recent_news`, no `culture`.
- `render()` output ≤ ~400 tokens (`estimate_tokens` from the matcher); empty fields omitted; sources never rendered.
- State layout: `COMPANY PROFILE:\n…\n\nJOB DESCRIPTION:\n{jd}`; without a profile it is exactly today's `JOB DESCRIPTION:\n{jd}`.
- Refresh overwrites every profile field and clears `edited_at`. Cancelled research never increments `fail_count`.
- `agentix` pinned with `=` to the version chosen in Task 0.
- Keys live in `secrets` (`exa`, `openrouter`), never sent to the frontend beyond `has_key` + last 4; `cvd-mcp` gets no tools for companies or secrets.
- Model refs keep the existing encoding (`ModelRef`): bare id = local, `<provider>:<id>` = remote. New setting key `research_model`; existing `active_model` stays the System One role.
- llama.cpp sidecar for chat models adds `--jinja` to `server_args`; decision models keep their args unchanged.
- Settings copy: roles are labelled "System One" and "Company research".

## Review Focus

1. JD with an empty `company` and no URL → no research, match scores immediately with the "Scored without company context" badge. (Test in Task 9.)
2. Two matches started for JDs of the same company → one agent run, both matches get the same profile. (Test in Task 8.)
3. No Exa key, or no research model configured → research fails fast with an actionable message ("Add your Exa key in Settings."), match proceeds without context; no backoff penalty. (Test in Task 8.)
4. Model emits malformed tool-call JSON or an unknown tool → the error is returned to the model as the tool result and the loop continues; it never panics. (Test in Task 7.)
5. Names differing only by accents, case, punctuation or legal suffix ("Nubank S.A. de C.V.", "nubank") → same row. (Test in Task 2.)

---

### Task 0: Spike — agentix, LFM2.5, Exa

No product code. Resolves the unknowns the later tasks consume.

**Files:**
- Create: `docs/superpowers/spike-2026-10-08-company-research.md`
- Create: `src-tauri/tests/fixtures/exa_search_response.json`

- [ ] **Step 1:** In a scratch crate, add the latest `agentix` and run its loop against `llama-server -m LFM2.5-2.6B-Q4_K_M.gguf --jinja` with one dummy tool. Record: exact version, the loop/tool API names, how to set base URL + key, how to cap rounds, how to cancel (token or drop), and whether a hook exists to rewrite message history between rounds. If no hook exists, record that Task 7 wraps the loop by driving one round at a time.
- [ ] **Step 2:** Same scratch run against OpenRouter `https://openrouter.ai/api/v1` with a tool-capable model. Record any request differences.
- [ ] **Step 3:** Download `LiquidAI/LFM2.5-2.6B-GGUF` Q4_K_M; record exact `hf_file`, `sha256`, `size_bytes`, and a `min_ram_gb`.
- [ ] **Step 4:** Call `POST https://api.exa.ai/search` with header `x-api-key` and body `{"query":"anthropic company","type":"auto","numResults":5,"contents":{"text":{"maxCharacters":500}}}`; save the response as the fixture. Record the result fields used (`results[].title/url/text`) and the 401/429 behaviour.
- [ ] **Step 5:** Pick the readable-text crate (`html2text` default; switch only if it fails on 3 real company pages) and record it.
- [ ] **Step 6: Commit** `docs: company research spike`.

### Task 1: Schema and company store

**Files:**
- Create: `src-tauri/migrations/0005_companies.sql` (exact SQL from the spec, plus `ALTER TABLE match_run ADD COLUMN company_researched_at TEXT;`)
- Create: `src-tauri/src/store/companies.rs`
- Modify: `src-tauri/src/store/mod.rs`, `src-tauri/src/store/types.rs` (`MatchRun` gets `company_researched_at: Option<String>`)
- Test: `src-tauri/src/store/tests.rs`

**Interfaces:**
- Produces (in `store/types.rs`):
  - `pub struct CompanyProfile { summary, industry, size, stage, headquarters, remote_policy, products: String, tech_stack: Vec<String>, sources: Vec<Source> }` (`Default`, `Serialize`, `Deserialize`)
  - `pub struct Source { url: String, title: String }`
  - `pub enum CompanyStatus { Pending, Ok, Failed }`
  - `pub struct Company { id: i64, name: String, normalized_name: String, domain: Option<String>, profile: CompanyProfile, status: CompanyStatus, researched_at: Option<String>, model_id: String, fail_count: i64, last_error: String, retry_after: Option<String>, edited_at: Option<String> }`
- Produces (on `Store`):
  - `find_company(domain: Option<&str>, normalized: &str) -> Result<Option<Company>>`: domain, then `normalized_name`, then `company_alias`.
  - `create_company(name: &str, normalized: &str, domain: Option<&str>) -> Result<Company>` (status `pending`)
  - `get_company(id: i64) -> Result<Company>`
  - `save_research(id: i64, canonical_name: &str, domain: Option<&str>, profile: &CompanyProfile, model_id: &str) -> Result<Company>`: sets status `ok`, `researched_at = now`, `fail_count = 0`, clears `edited_at`/`last_error`/`retry_after`.
  - `mark_research_failed(id: i64, error: &str, retry_after: &str) -> Result<()>` (increments `fail_count`)
  - `edit_company(id: i64, profile: &CompanyProfile) -> Result<Company>`: sets `edited_at = now`, leaves `researched_at`.
  - `add_alias(normalized: &str, company_id: i64) -> Result<()>` (insert-or-ignore)
  - `merge_company(from: i64, into: i64) -> Result<()>`: moves aliases and `job_description.company_id`, adds `from`'s `normalized_name` as an alias, deletes `from`.
  - `set_jd_company(jd_id: i64, company_id: Option<i64>) -> Result<()>`
  - `set_run_company(run_id: i64, researched_at: Option<&str>) -> Result<()>`

- [ ] **Step 1: Write failing tests** `company_lookup_order` (domain hit beats name hit; alias resolves), `save_research_clears_edit` (edit → `edited_at` set; save_research → `edited_at` None, profile overwritten, `fail_count` 0), `mark_failed_increments`, `merge_moves_aliases_and_jobs`, `run_records_company_researched_at`.
- [ ] **Step 2:** `cargo test store::` (from `src-tauri/`) → FAIL (missing functions).
- [ ] **Step 3:** Implement the migration and functions; lists stored as JSON text in `tech_stack_json` / `sources_json`, as `links_json` does.
- [ ] **Step 4:** Tests pass; `cargo test` overall still passes.
- [ ] **Step 5: Commit** `feat(store): company table, aliases and research persistence`.

### Task 2: Company keys

**Files:**
- Create: `src-tauri/src/company/mod.rs`, `src-tauri/src/company/key.rs`, `src-tauri/src/company/tests.rs`
- Modify: `src-tauri/src/lib.rs` (`mod company;`)

**Interfaces:**
- Produces: `pub fn normalize_name(name: &str) -> String`; `pub fn domain_from_url(url: &str) -> Option<String>` (lowercase host, `www.` stripped, `None` for empty/invalid and for job boards: `linkedin.com`, `indeed.com`, `greenhouse.io`, `lever.co`, `workable.com`, `ashbyhq.com`, `myworkdayjobs.com`, `glassdoor.com`, `occ.com.mx`, `computrabajo.com`).

- [ ] **Step 1: Write failing tests:**

```rust
assert_eq!(normalize_name("Nubank S.A. de C.V."), "nubank");
assert_eq!(normalize_name("  NUBANK  "), "nubank");
assert_eq!(normalize_name("Société Générale"), "societe generale");
assert_eq!(normalize_name("Google, LLC"), "google");
assert_eq!(normalize_name("Acme Inc."), "acme");
assert_eq!(normalize_name("Siemens GmbH"), "siemens");
assert_eq!(normalize_name(""), "");
assert_eq!(domain_from_url("https://www.stripe.com/jobs/123"), Some("stripe.com".into()));
assert_eq!(domain_from_url("https://www.linkedin.com/jobs/view/1"), None);
assert_eq!(domain_from_url("not a url"), None);
```

- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Implement. Accent stripping via a small mapping table, no new dependency. Suffix list: `inc, llc, ltd, corp, corporation, co, gmbh, ag, sa, sas, s a, s a de c v, sa de cv, s de rl de cv, bv, plc, srl, spa, pty`, stripped repeatedly from the end after punctuation→space.
- [ ] **Step 4:** Pass.
- [ ] **Step 5: Commit** `feat(company): name normalization and domain keys`.

### Task 3: Freshness and backoff policy

**Files:**
- Create: `src-tauri/src/company/policy.rs`
- Test: `src-tauri/src/company/tests.rs`

**Interfaces:**
- Produces: `pub const COMPANY_TTL: Duration` (30 days); `pub enum Need { Fresh, Research, BackingOff }`; `pub fn need(c: &Company, now: chrono::DateTime<chrono::Utc>) -> Need` (adds `chrono = "0.4"`; timestamps parsed from the store's UTC `YYYY-MM-DD HH:MM:SS`); `pub fn backoff(fail_count: i64) -> Duration` = 1 h × 2^(n−1), capped at 7 days.
- Rules: `Ok` and `researched_at + TTL > now` → `Fresh`; `retry_after > now` → `BackingOff`; else `Research`. An edited row past TTL is `Research`.

- [ ] **Step 1: Write failing tests** `fresh_within_ttl`, `stale_after_30_days`, `edited_rows_still_expire`, `failed_backs_off_then_retries`, `backoff_caps_at_7_days` (n=1 → 1 h, n=4 → 8 h, n=20 → 7 d).
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement; add `chrono` to `Cargo.toml`. **Step 4:** Pass.
- [ ] **Step 5: Commit** `feat(company): TTL and failure backoff policy`.

### Task 4: Profile in the System One state

**Files:**
- Modify: `src-tauri/src/matcher/questions.rs:128` (`jd_state`), `src-tauri/src/matcher/mod.rs` (`run_match`, `resume_match`, `execute` take `company: Option<&CompanyProfile>`)
- Create: `src-tauri/src/company/render.rs`
- Test: `src-tauri/src/matcher/tests.rs`, `src-tauri/src/company/tests.rs`

**Interfaces:**
- Consumes: `CompanyProfile` (Task 1).
- Produces: `pub fn render(name: &str, domain: Option<&str>, p: &CompanyProfile) -> String`; `pub fn jd_state(jd: &str, company: Option<&str>) -> String` where `company` is the rendered block.

- [ ] **Step 1: Write failing tests:**
  - `render_omits_empty_fields`: profile with only `industry = "Fintech"` → `"Name: Acme (acme.com)\nIndustry: Fintech"`.
  - `render_caps_length`: every field 5 000 chars → `estimate_tokens(out.len()) <= 400`.
  - `render_never_includes_sources`.
  - `jd_state_without_company_unchanged`: `jd_state("x", None) == "JOB DESCRIPTION:\nx"`.
  - `jd_state_puts_company_first`: starts with `"COMPANY PROFILE:\n"` and contains `"\n\nJOB DESCRIPTION:\nx"`.
  - `every_batch_shares_the_company_state` (matcher test with the fake engine recording `state`): all calls receive the identical state containing the profile.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement; per-field cap 300 chars, then trim whole fields from the bottom until under the token cap. Update existing callers to pass `None`. **Step 4:** Pass.
- [ ] **Step 5: Commit** `feat(matcher): company profile in System One state`.

### Task 5: Search providers (Exa)

**Files:**
- Create: `src-tauri/src/search/mod.rs`, `src-tauri/src/search/exa.rs`, `src-tauri/src/search/tests.rs`
- Modify: `src-tauri/src/commands.rs` (`known_provider` accepts `exa`; `set_provider_key` validates Exa by a 1-result query), `src-tauri/src/state.rs` (`search()` accessor rebuilt in `reload_providers`)

**Interfaces:**
- Produces:
  - `pub struct SearchHit { title: String, url: String, snippet: String }`
  - `#[async_trait] pub trait SearchProvider: Send + Sync { fn id(&self) -> &'static str; async fn search(&self, query: &str, count: u8) -> Result<Vec<SearchHit>, ProviderError>; }`
  - `pub struct ExaSearch::new(base_url: String, api_key: String)`; `pub const EXA: &str = "exa"`; `EXA_BASE = "https://api.exa.ai"`.
  - `AppState::search(&self) -> Result<Arc<dyn SearchProvider>, ProviderError>` → `Missing("Add your Exa key in Settings.")` when absent.

- [ ] **Step 1: Write failing wiremock tests** `exa_parses_fixture` (Task 0 fixture → hits with title/url/snippet), `exa_sends_api_key_header_and_body` (header `x-api-key`; body has `query`, `numResults: 5`, `contents.text.maxCharacters: 500`), `exa_401_is_unauthorized`, `exa_429_is_network_error_with_rate_limit_message`.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Pass.
- [ ] **Step 5: Commit** `feat(search): Exa search provider`.

### Task 6: Chat models and the research role

**Files:**
- Modify: `src-tauri/src/models/registry.rs` (`ModelSpec.kind: ModelKind`, add `LFM25_2_6B`), `src-tauri/src/engine/sidecar.rs` (`server_args(model, port, kind)`; public `ensure_started() -> Result<u16, EngineError>`), `src-tauri/src/providers/mod.rs`, `src-tauri/src/providers/openrouter.rs`, `src-tauri/src/state.rs`, `src-tauri/src/commands.rs`
- Test: `src-tauri/src/models/tests.rs`, `src-tauri/src/engine/sidecar_tests.rs`, `src-tauri/src/providers/tests.rs`

**Interfaces:**
- Produces:
  - `pub enum ModelKind { Decision, Chat }`; existing specs are `Decision`; `LFM25_2_6B { id: "lfm2.5-2.6b", label: "Liquid LFM2.5 2.6B (research)", hf_repo: "LiquidAI/LFM2.5-2.6B-GGUF", … values from Task 0 }`.
  - `pub struct ChatEndpoint { pub base_url: String, pub api_key: Option<String>, pub model: String }` (OpenAI-compatible; `base_url` ends in `/v1`).
  - `Provider::chat_endpoint(&self, model_id: &str) -> Result<ChatEndpoint, ProviderError>`; OpenRouter returns `{ "https://openrouter.ai/api/v1", key, id }`.
  - `Provider::list_chat_models()`: OpenRouter `GET /v1/models?supported_parameters=tools`.
  - `pub const RESEARCH_MODEL_KEY: &str = "research_model"`.
  - `AppState::research_endpoint(&self) -> Result<ChatEndpoint, AppError>`: local → a **second** `LlamaSidecar` slot (separate from `engine`) started with `ModelKind::Chat`, returns `http://127.0.0.1:{port}/v1`; remote → provider's endpoint; unset → `ModelMissing("Choose a Company research model in Settings.")`.
  - Commands: `set_research_model(id)`, `list_models` result gains `kind`, `list_remote_models` gains a `role: "decision" | "chat"` argument.

- [ ] **Step 1: Write failing tests** `chat_args_add_jinja` (Chat → contains `--jinja`; Decision → unchanged), `registry_kinds` (LFM is Chat, others Decision), `set_research_model_rejects_decision_models`, `research_endpoint_missing_message`, `openrouter_chat_endpoint`, `research_sidecar_independent_of_decision_sidecar` (switching `active_model` does not shut down the research slot).
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Pass.
- [ ] **Step 5: Commit** `feat(providers): chat models and research role`.

### Task 7: Research agent

**Files:**
- Create: `src-tauri/src/company/agent.rs`, `src-tauri/src/company/tools.rs`, `src-tauri/src/company/prompt.rs`
- Modify: `src-tauri/Cargo.toml` (`agentix = "=<Task 0 version>"`, readable-text crate)
- Test: `src-tauri/src/company/agent_tests.rs` (wiremock scripted `/v1/chat/completions` responses)

**Interfaces:**
- Consumes: `ChatEndpoint` (Task 6), `SearchProvider` (Task 5), `CompanyProfile` (Task 1).
- Produces:
  - `pub struct ResearchInput { pub name: String, pub domain: Option<String>, pub jd_snippet: String }` (snippet = first 1 500 chars of the JD)
  - `pub struct ResearchOutput { pub canonical_name: String, pub domain: Option<String>, pub profile: CompanyProfile }`
  - `pub enum ResearchStep { Searching { query: String }, Reading { domain: String }, Round { n: u32, max: u32 } }` (`Serialize`, tagged `"kind"`)
  - `pub enum ResearchError { Budget, Cancelled, Model(String), Search(ProviderError) }`
  - `pub async fn research(ep: &ChatEndpoint, search: Arc<dyn SearchProvider>, input: ResearchInput, cancel: CancellationToken, on_step: impl Fn(ResearchStep) + Send + Sync) -> Result<ResearchOutput, ResearchError>`
- Tools: `web_search { query }` → top 5 hits as `title — url\nsnippet`; `fetch_page { url }` → readable text truncated to 5 000 chars, 15 s timeout, 2 MB body cap, `http`/`https` only; `submit_profile { canonical_name, domain, summary, industry, size, stage, headquarters, remote_policy, products, tech_stack[], sources[{url,title}] }` ends the loop.
- System prompt (in `prompt.rs`): research only this company; fill only facts backed by a fetched or searched source; `""` for unknowns; call `submit_profile` exactly once.
- Context trimming: before each round, keep the last 3 `fetch_page` results verbatim and replace older ones with `"[page {url} read earlier — omitted]"`.
- Budget: after round 25 or 240 s, send one final user turn: "Budget exhausted. Call submit_profile now with what you have." If that does not produce `submit_profile`, return `Budget`.

- [ ] **Step 1: Write failing tests:**
  - `happy_path_search_fetch_submit` (scripted: search → fetch → submit) returns the profile; steps emitted in order `Searching`, `Reading{"acme.com"}`.
  - `malformed_tool_args_are_returned_to_model` (first response has invalid JSON args; second submits) → Ok, and request 2 contains a tool message with the parse error.
  - `unknown_tool_is_returned_to_model`.
  - `budget_forces_final_submit` (model never submits; 26th request contains the exhaustion message; scripted submit → Ok).
  - `budget_without_submit_is_error` → `Budget`.
  - `cancel_stops_loop` → `Cancelled`, no further requests.
  - `old_pages_are_trimmed` (5 fetches → request 6 contains 3 full pages + 2 placeholders).
  - `fetch_page_rejects_file_scheme`.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement on agentix as recorded in Task 0; if it has no history hook, drive single rounds and keep the history in `agent.rs`. Make `MAX_ROUNDS`/`WALL_CLOCK` overridable in tests through a `Budget` struct with these defaults. **Step 4:** Pass.
- [ ] **Step 5: Commit** `feat(company): research agent with search, fetch and submit tools`.

### Task 8: Research coordinator

**Files:**
- Create: `src-tauri/src/company/coordinator.rs`
- Modify: `src-tauri/src/state.rs` (`pub research: Arc<ResearchCoordinator>`)
- Test: `src-tauri/src/company/coordinator_tests.rs`

**Interfaces:**
- Consumes: Tasks 1–3, 7.
- Produces:
  - `pub const RESEARCH_EVENT: &str = "company://research"`; payload `{ company_id, event }` where `event` is `{stage: "queued" | "started" | "step" | "ready" | "failed" | "cancelled", step?, error?}`.
  - `pub struct ResearchCoordinator::new(max_concurrent: usize /* 1 */, emit: Emitter)`
  - `pub async fn ensure(&self, store: &Store, ep_fn, search_fn, company: Company, input: ResearchInput) -> Result<Company, ResearchError>`: returns at once when `need` is `Fresh` or `BackingOff`; otherwise joins the in-flight run for `company.id` or starts one (shared `futures::future::Shared` or a `tokio::sync::watch` per id). The run itself is spawned, so dropping the waiter does not stop it.
  - `pub fn cancel(&self, company_id: i64) -> bool`
  - On success: `save_research`; if output's normalized name/domain resolves to another row, `merge_company` into the older row and alias the new spelling. On `Model`/`Budget`/`Search` errors: `mark_research_failed` with `backoff`; on `Missing` errors and `Cancelled`: no state change.
- [ ] **Step 1: Write failing tests** (agent replaced by an injected `Fn` returning scripted results): `fresh_company_skips_research`, `concurrent_ensure_runs_once` (two waiters, one agent call, both get the profile), `concurrency_cap_queues_second_company` (second emits `queued` before `started`), `failure_sets_backoff`, `missing_key_does_not_back_off`, `cancel_leaves_row_unchanged`, `waiter_dropped_research_still_saves`, `merge_on_canonical_match`.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement; make `research` injectable via a boxed async fn field so tests avoid HTTP. **Step 4:** Pass.
- [ ] **Step 5: Commit** `feat(company): single-flight research coordinator`.

### Task 9: Match flow

**Files:**
- Modify: `src-tauri/src/commands.rs` (`start_match`, `spawn_run`, new commands), `src-tauri/src/matcher/mod.rs` (`Progress::Researching { company_id }`)
- Test: `src-tauri/src/commands_tests.rs`

**Interfaces:**
- Produces commands: `skip_research(run_id: i64, mode: "cancel" | "background")`, `rescore(run_id: i64) -> i64` (new run for the same JD), `get_company(id)`, `edit_company(id, profile)`, `refresh_company(id)` (ignores TTL and backoff, awaits the coordinator), `get_job_company(jd_id) -> Option<Company>`.
- `spawn_run` flow: resolve the company (`domain_from_url(jd.url)`, `normalize_name(jd.company)`, create if missing, `set_jd_company`) → if the name is empty and there is no domain, skip → emit `Progress::Researching` → `select!` over `coordinator.ensure(...)` and a per-run skip signal → on skip `cancel` calls `coordinator.cancel`, `background` just stops waiting → `set_run_company(run_id, researched_at if the profile is ok)` → score with `render(...)` or `None`.
- `MatchRun` serialization exposes `company_researched_at`; the frontend shows the badge when it is `null` and the run is done.

- [ ] **Step 1: Write failing tests:** `empty_company_scores_without_research` (fake engine, no coordinator calls, `company_researched_at` null), `fresh_profile_reaches_state` (fake engine records state containing `COMPANY PROFILE`), `skip_background_scores_and_research_finishes` (profile saved later), `skip_cancel_scores_and_research_cancelled`, `research_failure_scores_without_context`, `rescore_creates_new_run_with_current_profile`, `edit_then_refresh_overwrites`.
- [ ] **Step 2:** Run → FAIL. **Step 3:** Implement; register the new commands in the `ipc` module and `lib.rs` handler list. **Step 4:** Pass; `cargo clippy -- -D warnings` clean.
- [ ] **Step 5: Commit** `feat: block matches on company research with skip modes`.

### Task 10: Frontend

**Files:**
- Modify: `src/app/core/ipc/types.ts`, `commands.ts`, `events.ts`; `src/app/core/stores/models.store.ts`; `src/app/features/jobs/run-status-badge.component.ts`; `src/app/features/jobs/jobs-page.component.ts`; `src/app/features/models/model-cards.component.ts`, `remote-models.component.ts`; `src/app/features/settings/settings-page.component.ts`, `provider-key.component.ts`
- Create: `src/app/features/companies/company-panel.component.ts`, `src/app/features/companies/research-progress.component.ts`, `src/app/core/stores/company.store.ts`
- Test: `src/app/features/companies/companies.spec.ts`, `src/app/core/ipc/commands.spec.ts`

**Interfaces:**
- Consumes: Task 9 commands, `RESEARCH_EVENT` payloads, `MatchRun.company_researched_at`, `ModelInfo.kind`.

- [ ] **Step 1: Write failing vitest specs:**
  - `research progress shows "Researching Acme… step 7/25 · 1:42"` from a `step` event with `Round{n:7,max:25}` and a 102 s elapsed clock.
  - `cancel button calls skip_research(run, "cancel")`; `background button calls skip_research(run, "background")`.
  - `badge "Scored without company context" when a done run has null company_researched_at`; `Re-score` button visible only when the job's company is `ok`.
  - `refresh asks for confirmation when edited_at is set` and not otherwise.
  - `settings shows Exa key (provider-key with provider "exa") and two model pickers labelled "System One" and "Company research"`; the research picker lists only `kind === "chat"` local models plus remote chat models.
  - `commands.spec`: new command names and argument shapes.
- [ ] **Step 2:** `bun --bun ng test --watch=false` → FAIL.
- [ ] **Step 3:** Implement. The company panel opens from the job page and shows the fields, sources (links), `researched_at`, "edited by you" badge, Edit (form over the typed fields; tech stack as a chip list), Refresh. **Step 4:** Pass.
- [ ] **Step 5:** Launch the app (`run` skill) with an Exa key and LFM2.5 installed; run a match for a real JD; confirm the progress, both skip buttons, the badge and the panel.
- [ ] **Step 6: Commit** `feat(ui): company research progress, profile panel and settings`.
