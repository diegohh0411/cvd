# OpenRouter Provider — Design

Date: 2026-10-08

## Goal

Let the user run decisions on online System One models (e.g. OpenAI GPT-6 Luna Decisions, TypeSafe Jev) through OpenRouter when they configure an API key, alongside the existing local llama-server models. Introduce a provider abstraction that later online providers can plug into.

## Decisions

- Local and online models coexist; the user picks per selection (option B). No automatic fallback between them.
- MVP stores API keys in plaintext in a new `secrets` table (not the keychain). Keychain migration later changes only the store code.
- OpenRouter's model list is fetched live from `GET /api/v1/models?output_modalities=decisions` (plain endpoint, not `/models/user`).
- Decisions go to `POST https://openrouter.ai/api/v1/systemone`.
- The 128-question cap (`MAX_QUESTIONS`) stays for every provider.

## Unverified assumptions (check first, against the live API)

1. `output_modalities=decisions` filters to decision models and returns `id`, `name`, `context_length`, `pricing.prompt`.
2. `/api/v1/systemone` accepts `state` as a string (we send a JSON-encoded string today) and `criteria` values of `null` for choice questions. If not, `wire_request` gains a remote variant that sends `state` as an object.
3. `openai/gpt-6-luna-decisions` is served on `/api/v1/systemone` (its page mentions the "Decisions API"). If not, `ModelInfo` gets an endpoint override.

## Data

Migration `0004_secrets.sql`:

```sql
CREATE TABLE secrets (
  provider   TEXT PRIMARY KEY,
  api_key    TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

- One key per provider.
- Keys are never returned to the frontend; the UI gets `has_key` and the last 4 characters.
- `cvd-mcp` must not expose this table.

The selected model setting becomes `{provider, model_id}`. If the saved model is no longer listed, fall back to the default local model and notify the user.

## Interfaces

`DecisionEngine` (`engine/mod.rs`) is unchanged and remains the single inference trait.

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn requires_key(&self) -> bool;
    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError>;
    fn engine(&self, model_id: &str) -> Result<Box<dyn DecisionEngine>, ProviderError>;
}

pub struct ModelInfo {
    pub provider: ProviderId,
    pub id: String,
    pub display_name: String,
    pub context_length: Option<u32>,
    pub price_per_mtok_in: Option<f64>,
    pub remote: bool,
}
```

- `LocalProvider`: wraps `models/registry.rs` and downloaded GGUFs; `engine()` returns `LlamaSidecar`.
- `OpenRouterProvider { api_key, http }`: `list_models()` hits the models endpoint; `engine()` returns `HttpSystemOneEngine { base_url, api_key, model }`.
- `HttpSystemOneEngine`: generic `/v1/systemone` client reusing `engine/wire.rs`, adding `model` to the request. Future providers speaking the same protocol only supply a base URL.
- `ProviderRegistry` (app state): built from `secrets`; keyless providers are not registered. `all_models()` aggregates per provider, isolating failures (a failing provider is reported with an error, others still listed). `engine_for(provider, model_id)`. Remote lists cached in memory ~10 min; `refresh` bypasses cache.

## UI and commands

- Settings → Providers: OpenRouter row, paste key → validated with one `/models` call → saved. Shows "Key saved (…ab12)" and Remove.
- Model picker grouped by provider; online models show an "online" badge and input price.
- First selection of a remote model shows a one-time notice that CV and job text are sent to OpenRouter.
- Tauri commands: `set_provider_key`, `delete_provider_key`, `list_models { refresh?: bool }`, `get_selected_model`, `set_selected_model`.
- The matcher calls `registry.engine_for(selection)` then `decide_checked`; matcher logic is otherwise unchanged.

## Errors

New `EngineError` variants:

| Variant | Trigger | UI message |
|---|---|---|
| `Unauthorized` | 401 | Check your OpenRouter key |
| `InsufficientCredits` | 402 | Out of OpenRouter credits |
| `RateLimited` | 429 | Retry once honoring `Retry-After`, then fail |
| `Network` | transport failure | Network error |
| `ModelUnavailable` | 404/400 for unknown model | Model no longer available |

## Testing

- Wire: golden tests for request (with `model`) and parsing of a captured OpenRouter response fixture.
- `HttpSystemOneEngine`: `wiremock` tests for 200 and each error status.
- `OpenRouterProvider::list_models`: fixture test for query string and field mapping.
- `ProviderRegistry`: keyless provider hidden; one provider failing does not hide others.
- Secrets store: round-trip; not reachable from `cvd-mcp`.
- Manual smoke: one live call each to Jev, Luna, Mercury Decide (free).

## Out of scope

OS keychain storage, multiple keys per provider, offline persistence of remote model lists, non-OpenRouter online providers, `/models/user` filtering.
