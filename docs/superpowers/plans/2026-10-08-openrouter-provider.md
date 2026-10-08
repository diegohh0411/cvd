# OpenRouter Provider Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user pick online System One models from OpenRouter (listed live) next to local llama-server models, behind a provider abstraction that future online providers can implement.

**Architecture:** A `Provider` trait (`local`, `openrouter`) lists models and builds a `DecisionEngine`. `HttpSystemOneEngine` is a generic `/v1/systemone` client reusing `engine/wire.rs`. A `ProviderRegistry` in `AppState` replaces the direct `LlamaSidecar` construction in `engine_for`. API keys live in a new `secrets` table.

**Tech Stack:** Rust (Tauri 2, sqlx/SQLite, reqwest 0.13, async-trait, tokio), `wiremock` (new dev-dependency), Angular frontend.

**Spec:** `docs/superpowers/specs/2026-10-08-openrouter-provider-design.md`

## Global Constraints

- OpenRouter base URL: `https://openrouter.ai/api`. Decisions: `POST {base}/v1/systemone`. Model list: `GET {base}/v1/models?output_modalities=decisions` (plain endpoint, never `/models/user`).
- Auth header: `Authorization: Bearer <key>`.
- `MAX_QUESTIONS` stays 128 for all providers.
- Keys are stored plaintext in `secrets`, one row per provider; never sent to the frontend (only `has_key` and last 4 chars). `cvd-mcp` gets no tool touching `secrets`.
- No automatic fallback from a remote model to a local one during a run.
- Remote model lists cached in memory for 10 minutes; `refresh` bypasses the cache.
- **Model reference encoding (plan decision):** the existing `active_model` setting and `match_runs.model_id` column keep a single string. Local models stay bare ids (`clef-flash`); remote ones are `"<provider>:<remote id>"` (`openrouter:typesafe/jev-1.13`). Existing rows therefore parse as local with no migration. Remote ids contain `/` but never start with `local:`; split on the first `:` only.

## Review Focus

1. Saved active model is an OpenRouter model but the key was deleted → `engine_status` shows `error` with "Add your OpenRouter key", matching fails with that message, nothing panics. (Test in Task 4.)
2. Resuming an old run whose `model_id` is a bare local id after this change → still resolves to the local sidecar. (Test in Task 4.)
3. OpenRouter returns 200 with a body lacking `answers` (or an `error` object) → `EngineError::BadResponse`, not a panic. (Test in Task 2.)
4. Models endpoint unreachable while local models are installed → model list still returns the local models plus a per-provider error. (Test in Task 4.)
5. Key with surrounding whitespace pasted from the clipboard → trimmed before storage and validation. (Test in Task 5.)

---

### Task 0: Verify the live API and capture fixtures

Resolves the spec's three unverified assumptions. No product code.

**Files:**
- Create: `src-tauri/tests/fixtures/openrouter_models.json`
- Create: `src-tauri/tests/fixtures/openrouter_systemone_response.json`
- Create: `docs/superpowers/spike-2026-10-08-openrouter.md`

- [ ] **Step 1:** With the user's key in `OPENROUTER_API_KEY`, run `curl -s -H "Authorization: Bearer $OPENROUTER_API_KEY" 'https://openrouter.ai/api/v1/models?output_modalities=decisions'`. Save a trimmed copy (3–5 models, including `openai/gpt-6-luna-decisions` and `typesafe/jev-1.13`) to `openrouter_models.json`. Note the field names for id, name, context length and prompt price.
- [ ] **Step 2:** POST `tests/fixtures/decision_request.json` plus `"model": "typesafe/jev-1.13"` to `/api/v1/systemone`. Record whether string `state` and `null` criteria are accepted. Save the response to `openrouter_systemone_response.json`.
- [ ] **Step 3:** Repeat Step 2 with `"model": "openai/gpt-6-luna-decisions"`. Record whether it works on `/v1/systemone`.
- [ ] **Step 4:** Write the three findings to the spike doc. **If any assumption fails, stop and report to the user before Task 2**: a rejected string `state` means `wire_request` needs a remote variant, and Luna off `/v1/systemone` means `ModelInfo` needs an endpoint override.
- [ ] **Step 5:** Commit: `git commit -m "docs(spike): verify OpenRouter systemone and models API"`.

### Task 1: `secrets` table and store methods

**Files:**
- Create: `src-tauri/migrations/0004_secrets.sql` (schema verbatim from spec)
- Modify: `src-tauri/src/store/mod.rs` (new `// ---- secrets ----` section after settings)
- Test: `src-tauri/src/store/tests.rs`

**Interfaces:**
- Produces:
  - `Store::get_secret(&self, provider: &str) -> Result<Option<String>>`
  - `Store::set_secret(&self, provider: &str, api_key: &str) -> Result<()>` (upsert; `updated_at` = current UTC as RFC 3339 via SQLite `strftime('%Y-%m-%dT%H:%M:%SZ','now')`)
  - `Store::delete_secret(&self, provider: &str) -> Result<()>`

- [ ] **Step 1:** Write `secret_roundtrip`: get on an empty store → `None`; set `"openrouter"`/`"sk-or-1"` → get returns `Some("sk-or-1")`; set `"sk-or-2"` → get returns `"sk-or-2"`; delete → `None`.
- [ ] **Step 2:** Run `cargo test --manifest-path src-tauri/Cargo.toml secret_roundtrip`. Expect it to fail to compile.
- [ ] **Step 3:** Add the migration and the three methods, following `get_setting`/`set_setting`.
- [ ] **Step 4:** Run the test. Expect PASS.
- [ ] **Step 5:** Run `grep -n secret src-tauri/src/mcp/mod.rs`. Expect no matches.
- [ ] **Step 6:** Commit: `feat(store): secrets table`.

### Task 2: `HttpSystemOneEngine` and new error variants

**Files:**
- Create: `src-tauri/src/engine/http.rs`
- Modify: `src-tauri/src/engine/types.rs`, `src-tauri/src/engine/mod.rs` (`mod http; pub use http::HttpSystemOneEngine;`), `src-tauri/Cargo.toml` (`[dev-dependencies] wiremock = "0.6"`)
- Test: `src-tauri/src/engine/http_tests.rs` (wired like `sidecar_tests.rs`)

**Interfaces:**
- Produces:
  - `EngineError::{Unauthorized, InsufficientCredits, RateLimited, Network(String), ModelUnavailable(String)}`, with display strings from the spec's Errors table (`Unauthorized` → "Check your OpenRouter key", etc.).
  - `HttpSystemOneEngine::new(base_url: String, api_key: String, model: String) -> Self`
  - `impl DecisionEngine for HttpSystemOneEngine`

- [ ] **Step 1:** Write tests against a `wiremock::MockServer`:
  - `sends_model_and_bearer`: a mock matching `POST /v1/systemone`, header `authorization: Bearer k`, and a body containing `"model":"m"` returns the Task 0 response fixture. `decide` returns the same answers as `parse_wire_response(fixture)`.
  - `maps_status_codes`: 401 → `Unauthorized`, 402 → `InsufficientCredits`, 404 → `ModelUnavailable(_)`, 400 with `{"error":{"message":"unknown model"}}` → `ModelUnavailable(_)`.
  - `retries_429_once`: the first response is 429 with `Retry-After: 0`, the second is 200 → `Ok`. A test with two 429s → `RateLimited`, and the mock records exactly 2 requests.
  - `bad_body_is_bad_response` (Review Focus 3): 200 with `{}` → `BadResponse(_)`.
  - `network_error`: point the engine at `http://127.0.0.1:1` → `Network(_)`.
- [ ] **Step 2:** Run `cargo test --manifest-path src-tauri/Cargo.toml http_tests`. Expect a compile failure.
- [ ] **Step 3:** Implement. The body is `wire_request(state, questions)` with `"model"` inserted. Parse with `parse_wire_response`. A 400 counts as `ModelUnavailable` only when the error message mentions the model; otherwise it's `BadResponse`. Honor `Retry-After` in seconds, capped at 10 s.
- [ ] **Step 4:** Run the tests. Expect PASS.
- [ ] **Step 5:** Commit: `feat(engine): HTTP systemone engine`.

### Task 3: `Provider` trait, `ModelRef`, and both providers

**Files:**
- Create: `src-tauri/src/providers/mod.rs` (trait, `ModelRef`, `RemoteModel`, `ProviderError`), `src-tauri/src/providers/local.rs`, `src-tauri/src/providers/openrouter.rs`
- Modify: `src-tauri/src/lib.rs` (`mod providers;`)
- Test: `src-tauri/src/providers/tests.rs`

**Interfaces:**
- Produces:
  - `pub struct ModelRef { pub provider: String, pub id: String }`
    - `ModelRef::parse(s: &str) -> ModelRef`: no `:` → provider `"local"`; otherwise split on the first `:`.
    - `ModelRef::encode(&self) -> String`: the inverse, giving a bare id for local.
  - `pub struct RemoteModel { pub provider: String, pub id: String, pub display_name: String, pub context_length: Option<u32>, pub price_per_mtok_in: Option<f64> }` (serde `Serialize`). This replaces the spec's `ModelInfo` name, which already exists in `commands.rs` for local models.
  - `#[async_trait] pub trait Provider: Send + Sync { fn id(&self) -> &'static str; async fn list_models(&self) -> Result<Vec<RemoteModel>, ProviderError>; fn engine(&self, model_id: &str) -> Result<Arc<dyn DecisionEngine>, ProviderError>; }`. The `requires_key` method from the spec is dropped because the registry only registers keyed providers (YAGNI).
  - `ProviderError::{Unauthorized, Network(String), BadResponse(String), Missing(String)}`
  - `OpenRouterProvider::new(base_url: String, api_key: String) -> Self`. The constant `OPENROUTER_BASE = "https://openrouter.ai/api"` lives here.
  - `OpenRouterProvider::validate_key(&self) -> Result<(), ProviderError>`: one call to `list_models`.
  - `LocalProvider` is used only to build sidecar engines (`list_models` returns `Ok(vec![])`, because local models keep the existing `commands::list_models`). It wraps `AppState::installed_path` logic plus `EngineSource`, so it holds the same data `AppState::engine_for` uses today.
- [ ] **Step 1:** Write the tests:
  - `model_ref_parse`:
    - `"clef-flash"` → `("local","clef-flash")`
    - `"openrouter:typesafe/jev-1.13"` → `("openrouter","typesafe/jev-1.13")`
    - `encode` round-trips both.
  - `openrouter_lists_decision_models`: a wiremock `GET /v1/models` with query `output_modalities=decisions` and the bearer header returns the Task 0 fixture. The result contains `typesafe/jev-1.13` with a price per million input tokens equal to the fixture's per-token prompt price × 1e6.
  - `openrouter_401_is_unauthorized`
- [ ] **Step 2:** Run `cargo test --manifest-path src-tauri/Cargo.toml providers`. Expect a compile failure.
- [ ] **Step 3:** Implement. `OpenRouterProvider::engine` returns `HttpSystemOneEngine::new(base, key, model_id)`.
- [ ] **Step 4:** Run the tests. Expect PASS.
- [ ] **Step 5:** Commit: `feat(providers): provider trait, local and OpenRouter`.

### Task 4: `ProviderRegistry` in `AppState`

**Files:**
- Create: `src-tauri/src/providers/registry.rs`
- Modify: `src-tauri/src/state.rs` (`engine_for`, `engine_status`, `installed_path` callers), `src-tauri/src/commands.rs` (`start_match`, `spawn_run`, and `set_active_model` call `engine_for` with the encoded string, which is unchanged)
- Test: `src-tauri/src/providers/tests.rs`, `src-tauri/src/commands_tests.rs`

**Interfaces:**
- Consumes: Task 1 store methods; Task 3 `Provider`, `ModelRef`, `OpenRouterProvider`.
- Produces:
  - `ProviderRegistry::load(store: &Store, openrouter_base: &str) -> Result<Self>`: registers `openrouter` only if `get_secret("openrouter")` is `Some`.
  - `ProviderRegistry::remote_models(&self, refresh: bool) -> Vec<ProviderModels>`, where `pub struct ProviderModels { pub provider: String, pub models: Vec<RemoteModel>, pub error: Option<String> }` (Serialize), cached for 10 minutes per provider.
  - `ProviderRegistry::engine(&self, r: &ModelRef) -> Result<Arc<dyn DecisionEngine>, ProviderError>`: `Missing("Add your OpenRouter key in Settings.")` when the provider isn't registered.
  - `AppState` holds `providers: tokio::sync::RwLock<ProviderRegistry>` and `pub async fn reload_providers(&self) -> Result<(), AppError>`. It also gets `openrouter_base: String` (production uses `OPENROUTER_BASE`; tests use the wiremock URI).
  - `AppState::engine_for(&self, model: &str)` keeps its name. It parses `ModelRef`; for `local` it keeps today's sidecar path, and otherwise it delegates to the registry. `EngineSource::Fixed` still short-circuits everything for tests. The caller in `commands.rs` that runs it synchronously now needs `.await`, because the registry lock is async.
  - `engine_status` reports a remote ref as `idle`, or as `error` with the `Missing` message when the provider isn't registered.
- [ ] **Step 1:** Write the tests:
  - `registry_hides_keyless_provider`: `load` on an empty store → `engine(openrouter:x)` returns `Missing`.
  - `registry_isolates_failures` (Review Focus 4): the OpenRouter mock returns 500 → `remote_models` gives one entry with `error: Some(_)` and no panic.
  - `remote_models_cached`: two calls → the mock receives 1 request. `refresh=true` → 2 requests.
  - In `commands_tests.rs`:
    - `missing_key_status` (Review Focus 1): `active_model = "openrouter:typesafe/jev-1.13"` with no secret → `engine_status().state == "error"`, the error mentions "OpenRouter key", and `start_match` returns `Err`.
    - `bare_local_id_still_local` (Review Focus 2): `ModelRef::parse` of a stored legacy run `model_id` resolves to the local path (assert `installed_path` is consulted, i.e. a `ModelMissing` error for a non-installed model, not `Missing`).
- [ ] **Step 2:** Run `cargo test --manifest-path src-tauri/Cargo.toml`. Expect the new tests to fail.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the full suite with `cargo test --manifest-path src-tauri/Cargo.toml`. Expect all tests to pass, including existing ones.
- [ ] **Step 5:** Commit: `feat(state): route engines through provider registry`.

### Task 5: Commands and error mapping

**Files:**
- Modify: `src-tauri/src/commands.rs` (new functions, `ipc` wrappers, `AppError` mapping), `src-tauri/src/lib.rs` (register commands)
- Test: `src-tauri/src/commands_tests.rs`

**Interfaces:**
- Consumes: Task 4 `AppState::reload_providers`, `ProviderRegistry::remote_models`; Task 3 `OpenRouterProvider::validate_key`.
- Produces (IPC names are the contract for Task 6):
  - `set_provider_key(provider: String, key: String) -> Res<KeyStatus>`: trims the key, validates it with `validate_key` against `st.openrouter_base` (only `"openrouter"` is accepted; anything else → `NotFound`), stores it, reloads providers, and emits `ENGINE_EVENT`.
  - `delete_provider_key(provider: String) -> Res<()>`: deletes and reloads.
  - `provider_key_status(provider: String) -> Res<KeyStatus>`, where `pub struct KeyStatus { pub has_key: bool, pub last4: Option<String> }`.
  - `list_remote_models(refresh: bool) -> Res<Vec<ProviderModels>>`
  - `set_active_model(id)`: for a remote ref it skips `installed_path` and requires the provider to be registered; local behavior is unchanged.
  - `AppError` gets new variants `ProviderAuth(String)`, `ProviderCredits(String)`, `ProviderUnavailable(String)` with matching `kind` strings. `EngineError` maps: `Unauthorized` → `ProviderAuth`, `InsufficientCredits` → `ProviderCredits`, `RateLimited | Network | ModelUnavailable` → `ProviderUnavailable`.
- [ ] **Step 1:** Write the tests:
  - `set_key_trims_and_validates` (Review Focus 5): key `"  sk-or-abcd1234 \n"` against a wiremock that only accepts `Bearer sk-or-abcd1234` → `Ok(KeyStatus { has_key: true, last4: Some("1234") })`, and the stored secret equals the trimmed key.
  - `set_key_rejects_invalid`: the mock returns 401 → `Err(ProviderAuth)` and nothing is stored.
  - `delete_key_unregisters`
  - `set_active_remote_requires_key`
- [ ] **Step 2:** Run the tests. Expect FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run `cargo test --manifest-path src-tauri/Cargo.toml`. Expect all tests to pass.
- [ ] **Step 5:** Commit: `feat(commands): provider key and remote model commands`.

### Task 6: Frontend

**Files:**
- Modify: `src/app/core/ipc/types.ts`, `src/app/core/ipc/commands.ts`, `src/app/core/ipc/errors.ts` (new kinds), `src/app/features/settings/settings-page.component.ts`, `src/app/features/models/models-page.component.ts`
- Create: `src/app/features/settings/provider-key.component.ts`, `src/app/features/models/remote-models.component.ts`
- Test: `src/app/core/ipc/commands.spec.ts` (follow existing patterns)

**Interfaces:**
- Consumes: Task 5 IPC names and `KeyStatus`, `ProviderModels`, `RemoteModel` shapes (snake_case fields as serialized).
- [ ] **Step 1:** Add the types and command wrappers, plus spec tests asserting each wrapper invokes the right command name with the right args.
- [ ] **Step 2:** Settings → "Providers" section with `provider-key.component`:
  - A password input and a Save button.
  - Once saved, it shows "Key saved (…{last4})" with a Remove button.
  - Errors use the existing error display, and `ProviderAuth` shows "Check your OpenRouter key".
- [ ] **Step 3:** Models page: when `provider_key_status('openrouter').has_key` is true, render `remote-models.component` below the local cards:
  - Heading "OpenRouter (online)", a Refresh button, and one row per model with name, id and input price in "$X / M tokens" format.
  - A "Use" button calls `set_active_model('openrouter:' + id)`.
  - Any per-provider `error` is shown inline.
- [ ] **Step 4:** Add the one-time notice. The first time a remote model is chosen, show a confirm dialog: "Your CV entries and job descriptions will be sent to OpenRouter and the model's provider to score them." Store the acknowledgement via `set_ui_setting('remote_notice_ack','1')`. If the user cancels, the model is not activated.
- [ ] **Step 5:** Check the engine footer (`shell/engine-footer.component.ts`): it shows the encoded id, so render the part after `openrouter:` with an "online" tag.
- [ ] **Step 6:** Run `bun run test` and `bun run build`. Expect both to pass.
- [ ] **Step 7:** Commit: `feat(ui): OpenRouter key settings and online model picker`.

### Task 7: Manual smoke test

- [ ] **Step 1:** Run `bun tauri dev`. Add the key in Settings, open the model list, and check that Luna, Jev and Mercury Decide (free) appear.
- [ ] **Step 2:** Run one match each with `openrouter:typesafe/jev-1.13`, `openrouter:openai/gpt-6-luna-decisions` and `openrouter:inception/mercury-decide:free`, and confirm scores appear. Note that the last id itself contains `:`, which `ModelRef::parse` handles by splitting on the first `:` only.
- [ ] **Step 3:** Remove the key and check that the footer shows the "Add your OpenRouter key" error. Switch back to a local model and confirm a match runs.
- [ ] **Step 4:** Record the results in the spike doc and commit.
