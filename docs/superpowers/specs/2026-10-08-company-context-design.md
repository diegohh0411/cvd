# Company context for System One — design

Date: 2026-10-08 · Status: draft

## Goal

Every System One match is scored with researched context about the hiring company.
Profiles are cached per company for 30 days. A cache miss blocks the match while a
local research agent searches the web, with UI feedback and escape hatches.

## Decisions

| Topic | Decision |
|---|---|
| Blocking | Research blocks System One. User can **Cancel research & proceed** or **Continue in background & proceed**. |
| Agent loop | `agentix` (pinned exact version), running in the Tauri backend. |
| Research LLM | llama.cpp first (LFM2.5-2.6B), OpenRouter later. OpenAI-compatible `/v1/chat/completions` with tools. |
| Search | `SearchProvider` trait, Exa first. |
| Key | Domain → normalized name → alias. No embeddings. |
| Profile | Typed columns. Manual edits allowed; refresh overwrites them. |
| TTL | 30 days from `researched_at`, checked in code. Edits do not pause it. |
| Budget | 25 tool rounds, 4 min wall clock. |
| Injection | Profile goes into the System One `state`, ahead of the JD. |

## Data — `0005_companies.sql`

```sql
CREATE TABLE company (
    id               INTEGER PRIMARY KEY,
    name             TEXT NOT NULL,
    normalized_name  TEXT NOT NULL UNIQUE,
    domain           TEXT UNIQUE,
    summary          TEXT NOT NULL DEFAULT '',
    industry         TEXT NOT NULL DEFAULT '',
    size             TEXT NOT NULL DEFAULT '',
    stage            TEXT NOT NULL DEFAULT '',
    headquarters     TEXT NOT NULL DEFAULT '',
    remote_policy    TEXT NOT NULL DEFAULT '',
    products         TEXT NOT NULL DEFAULT '',
    tech_stack_json  TEXT NOT NULL DEFAULT '[]',
    sources_json     TEXT NOT NULL DEFAULT '[]',
    status           TEXT NOT NULL DEFAULT 'pending'
                     CHECK (status IN ('pending','ok','failed')),
    researched_at    TEXT,
    model_id         TEXT NOT NULL DEFAULT '',
    fail_count       INTEGER NOT NULL DEFAULT 0,
    last_error       TEXT NOT NULL DEFAULT '',
    retry_after      TEXT,
    edited_at        TEXT
);

CREATE TABLE company_alias (
    normalized_name TEXT PRIMARY KEY,
    company_id      INTEGER NOT NULL REFERENCES company(id) ON DELETE CASCADE
);

ALTER TABLE job_description ADD COLUMN company_id INTEGER REFERENCES company(id) ON DELETE SET NULL;
ALTER TABLE match_run ADD COLUMN company_researched_at TEXT;  -- NULL = scored without context
```

- **Normalization:** lowercase, strip accents, legal suffixes (Inc, LLC, S.A. de C.V., GmbH, …) and punctuation, collapse whitespace.
- **Resolution:** JD URL domain → `normalized_name` → `company_alias` → new `pending` row.
- **Merge:** the agent returns `canonical_name` + `domain`; if they hit an existing row, the new spelling becomes an alias.
- **Edits** set `edited_at`. Refresh overwrites all profile fields, clears `edited_at` and asks for confirmation first if it was set.
- **Failures** bump `fail_count` and set `retry_after` (backoff). Cancelled research does not count as a failure.

## Research agent

- **Input:** company name as written, JD URL/domain, a JD snippet for telling similar names apart.
- **Tools:**
  - `web_search(query)` → `SearchProvider` (Exa).
  - `fetch_page(url)` → `reqwest` + readability extraction, truncated to ~4–6k chars.
  - `submit_profile({canonical_name, domain, summary, industry, size, stage, headquarters, remote_policy, products, tech_stack[], sources[]})`. Calling it ends the loop.
- **Rules:** only sourced facts; `""` for unknowns.
- **Budget:** 25 rounds, 4 min. On exhaustion, ask once for `submit_profile` with what it has, else mark `failed`.
- **Context management:** keep the last few `fetch_page` results in full and collapse older ones, sized against the server's context window (`/props`). Verify agentix exposes a hook for this; wrap the loop if not.
- **Concurrency:** single-flight per company (shared future in `AppState`); a global cap (default 1 on llama.cpp) for different companies.

## System One integration

```rust
pub fn jd_state(jd: &str, company: Option<&CompanyProfile>) -> String
```

- `COMPANY PROFILE:` block before `JOB DESCRIPTION:`. Empty fields omitted, `render()` capped at ~400 tokens. No sources.
- Shared across all batches in a run, so llama.cpp can reuse the cached prefix. `batches()` already counts `state` against the token budget.
- `match_run.company_researched_at` records which profile was used. If it differs from the current profile, **Re-score** starts a fresh run, since the matcher otherwise reuses existing answers.

## Providers and models

- `DecisionEngine` (`/v1/systemone`) is unchanged.
- Each provider config gains a **chat role**: base URL, model and key (keys go in the secrets table). Settings has a model picker per role: *System One* and *Company research*.
- Each role picks its provider on its own: both local, both on OpenRouter (fully online, no models loaded), or mixed. Users trade RAM for privacy by picking smaller local models or going online.
- When a role is local, the llama.cpp sidecar serves it; with both local, it loads a decision model and a chat model.
- `ModelSpec` gains a `kind: Decision | Chat`. Add `LiquidAI/LFM2.5-2.6B-GGUF` (Q4_K_M); sha256 and size get filled in during implementation. Started with `--jinja` so tool calls parse.

## UI

- **Match progress:** "Researching Acme… step 7/25 · 1:42" with a live step log, driven by Tauri events (`company_research_queued`, `started`, `step`, `profile_ready`, `failed`).
- **Buttons while researching:** *Cancel research & proceed*, *Continue in background & proceed*.
- **Badge:** "Scored without company context", plus **Re-score** once a profile exists or has changed.
- **Company profile panel:** fields, sources, `researched_at`, "edited by you" badge, Edit, Refresh.
- **Settings:** Search providers (Exa key with a test button); LLM providers and the per-role model pickers.

## Open questions

- LFM2.5-2.6B's reliability over 25 tool rounds: test the loop against it first.
- LFM Open License v1.0 has a revenue threshold for commercial use. Check the terms before shipping.
