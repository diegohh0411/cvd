# AI Models page, TypeSafe provider, multi-key connections

## Problem

`/ai/roles` makes you pick a provider on a tab strip, then a model from a radio list, and sends you to a separate Providers page for keys. The tab list is hardcoded in `role-card.component.ts`, `ProviderKind` is a closed union, and the page is still called "Roles" although the two slots are now a decision model (scoring, analysis) and a chat model (company research and summaries).

## Goals

1. One page, **AI Models**, replacing Roles + Providers.
2. Two slots with plain names: **Fast Model** (was System One, decision kind) and **Smart Model** (was Company research, chat kind with tool calling).
3. One grouped, searchable model picker per slot instead of provider tabs.
4. Add **TypeSafe** (Jev) as a direct provider for the Fast slot, so existing TypeSafe credits can be used.
5. Several keys per provider ("Personal", "Work"), without migrating stored data.
6. Adding a provider later is one registry entry in the frontend and one Rust module.

Non-goals: new local models, Ollama/LM Studio, chat support for SubQ or TypeSafe (neither generates text).

## What exists today (do not regress)

Backend (`src-tauri/src`):

- `providers/mod.rs`: `Provider` trait (`list_models`, `list_chat_models`, `engine`, `chat_endpoint`); `ModelRef::parse` splits on the **first** `:` only because remote ids contain `:` (`inception/mercury-decide:free`). Covered by `model_ref_parse`.
- `providers/registry.rs`: built from `store.get_secret(<provider>)`; per-provider 10 min model cache keyed `<id>` / `<id>#chat`; one failing provider never hides the others.
- `engine/http.rs`: `HttpSystemOneEngine` posts `{base}/v1/systemone` with a bearer key. OpenRouter and SubQ both use it (OpenRouter's non-alpha `/api/v1/systemone`; the `alpha/decisions` path appears only in OpenRouter's SDK sample and is not used here). Status mapping: 401/403 unauthorized, 402 credits, 429 rate-limited (one retry honouring `Retry-After`), 404 and `does not exist` model unavailable, `token limit` context overflow. Error text read from `/error/message` or `/detail/message`.
- Settings keys `active_model` and `research_model` hold a bare id (local) or `<provider>:<id>`.
- `commands.rs`: `known_provider` allows `openrouter | subq | exa`; `set_provider_key` validates against the provider before saving; `set_active_model` / `set_research_model` call `providers().check`.
- SubQ is the template: decision-only, `list_chat_models` returns `[]`, `chat_endpoint` errors with a hint, per-request question cap.

Frontend (`src/app`):

- Routes `/ai` → `/ai/roles`, `/ai/models` (local downloads), `/ai/providers`; `/models?tab=` redirect via `modelsRedirect`. `shell.spec.ts` pins the sidebar NAV links and the engine footer href (`/ai/roles`).
- `role-options.ts`: `providerOf`, `localOptions`, `onlineOptions`, `roleStatus`, `REMOTE_NOTICE_SETTING` (`ui.remote_notice_ack`) data-sharing notice shown before the first online pick.
- `role-card.component.ts`: switching view never changes the active model; a pick in flight keeps the previous selection until it settles; failed switch keeps the old selection and toasts; engine Restart re-activates the active model; engine log shown in a collapsible block.
- Onboarding reuses `model-cards.component.ts`; a finished download auto-selects when the slot has no model.
- `ai.spec.ts` covers all of the above and must keep passing (updated only for renamed labels/routes).

## Design

### Page

`/ai` becomes the **AI Models** page. Sections, top to bottom:

1. **Fast Model** slot card: subtitle "Scores your library against each job and sorts results." Status badge (Ready / Needs setup / Error), a single picker button showing the active model and its provider, runtime line + Restart + engine log (unchanged behavior).
2. **Smart Model** slot card: subtitle "Researches companies and writes summaries." Same picker; requirement row for the Exa key.
3. **Connections**: one row per provider.
   - Local (llama.cpp): runtime state, RAM, installed count, link to Downloads.
   - Online providers (OpenRouter, SubQ, TypeSafe): list of connections (label + `…last4`), Add key, Replace, Remove, each with its own status.
   - Tools: Exa.

Picker (popover with search): groups by connection ("Local", "OpenRouter · Personal", "TypeSafe"), rows show label, size or price, and an Online marker. Filtered by slot kind, so Jev never appears under Smart Model. Providers without a key appear as a collapsed "Connect TypeSafe…" row that opens the key field inline. Uninstalled local models keep the "Download" link. Picking an online model shows the existing data-sharing notice first when not yet acknowledged.

### Routes

| Route | Content |
|---|---|
| `/ai` | AI Models page (new) |
| `/ai/roles`, `/ai/providers` | redirect to `/ai` |
| `/ai/downloads` | local model library (page formerly at `/ai/models`) |
| `/ai/models` | redirect to `/ai/downloads` |
| `/models?tab=engine` | `/ai` |
| `/models?tab=<other>` | `/ai/downloads` |

Sidebar AI group: `AI Models` (`/ai`) and `Downloads` (`/ai/downloads`). Engine footer links to `/ai`.

### Multi-key without a migration

A **connection** is `{ id, provider, label }`. The connection `id` takes the place of the provider segment in a model ref, so `ModelRef` parsing, `active_model` and `research_model` stay as they are.

- The first connection of a provider keeps the bare provider id (`openrouter`, `subq`, `typesafe`, `exa`). Existing secrets and stored model refs therefore keep working with no migration.
- Extra connections get ids `<provider>~<n>` (`openrouter~2`). Refs look like `openrouter~2:deepseek/x:free`; the first-`:` split still holds because the id contains no `:`.
- Labels and the connection list live in one settings row (`connections`, JSON). A provider with a secret but no row is shown as a single connection labelled with the provider name.
- `ProviderRegistry::load` creates one `Provider` per connection id; the provider kind decides the implementation and base URL.
- Commands gain a connection argument: `set_provider_key(provider, key, label?, connection?)`, `delete_provider_key(connection)`, `provider_key_status(connection)`, plus `list_connections()`. Existing callers that pass only a provider keep working (connection defaults to the provider id).
- Removing a connection that is the active model's connection leaves the slot in "Needs setup" with the existing reason text, never silently switching models.

### TypeSafe provider

Verified against docs.typesafe.ai (Oct 2026):

- `POST https://api.typesafe.ai/v1/systemone`, bearer key, same request/response shape as the existing wire format, so `HttpSystemOneEngine` is reused as is.
- `GET /v1/models` returns `{ models: [{ name, description, release_date }] }` (aliases `jev-latest`, `jev-preview`); used for model listing and key validation (401 on a bad key).
- $0.042 / M input tokens, output free; default context 64k per request / 32k per question; no question-count cap (the engine's `MAX_QUESTIONS` clamp still applies); choice max 255 options.
- Errors: 400 with `max_tokens_exceeded` (context), 401, 413, 422, 429, 529 (overloaded, retry).
- Decision-only: `list_chat_models` is empty and `chat_endpoint` errors with "pick an OpenRouter model", as SubQ does.
- Model refs are `typesafe:jev-latest` / `typesafe:jev-1.14.0`. Versioned ids are accepted by the API even when `/v1/models` does not list them.

Engine changes (shared, additive):

- Map `400` bodies containing `max_tokens_exceeded` to `ContextOverflow` alongside the OpenRouter `token limit` match.
- Map `529` to a retryable provider-unavailable error (one retry like 429).
- Read error text from `/error/message`, `/detail/message`, then `/message`; TypeSafe's error body shape is not documented, so this must be confirmed with a real key before release.

### Frontend registry

`provider-registry.ts` replaces the hardcoded `tabs` and the `ProviderKind` union:

```ts
interface ProviderDef { id; name; keyPlaceholder; keyHint; slots: Slot[]; online: boolean }
```

`ProviderId` widens to `string` at the IPC edge; `providerOf(ref)` resolves a connection id back to its provider through the connection list. `roleStatus` takes "does the active model's connection have a key" instead of one boolean per provider.

### Error handling

Unchanged from the Roles page: a failed switch keeps the previous model and toasts; a failed online list shows inline with Retry and never affects other groups; an engine error turns the Fast badge to Error and shows the log.

## Testing

- Rust: `typesafe_lists_models` (wiremock `/v1/models`), `typesafe_401_is_key_rejected`, `typesafe_engine_posts_systemone` (path, bearer, model), chat endpoint refusal, 529 retry, `max_tokens_exceeded` mapping, registry builds one provider per connection, `model_ref_parse` extended with `openrouter~2:` refs, secrets for existing bare ids still load.
- Frontend: existing `ai.spec.ts` cases ported to the new page (status, notice before first online pick, failed switch keeps selection, restart, engine log); picker groups and filters by slot; Jev absent from the Smart Model picker; routes spec for the new redirects; `shell.spec` NAV and footer href updated.
- Manual: real TypeSafe key against `/v1/models` and one `/v1/systemone` call from a job match.

## Rollout

Built on `feat/ai-models-page-typesafe`, not yet committed or released. Notes from implementation:

- Connection commands are additive: `list_connections`, `add_connection(provider, label, key)`, `rename_connection(id, label)`. `set_provider_key`, `delete_provider_key` and `provider_key_status` keep their signatures and accept a connection id, so the model refs, secrets and settings stored today need no migration.
- `ProviderRegistry::load` keeps its two-base signature (tests and callers); `load_with` adds the TypeSafe base.
- `ProviderModels` gained `label`; its `provider` field is now the connection id.
- Deep links that used to land on the downloads page (`ModelMissing`, match blocker, new job) now go to `/ai`, where any model can be chosen or connected.
- Open items: confirm TypeSafe's error body shape and run a real `/v1/systemone` call with a live key; no live smoke test exists for TypeSafe yet (OpenRouter has `live_openrouter_smoke`).
