# Settings & AI redesign

## Problem

AI configuration is split across Settings (provider keys, role dropdowns) and the Models page (local cards with "Use" buttons, OpenRouter list, Engine tab). The same choice can be made in two places, the pages link back and forth, and the research dropdown is empty with no hint when nothing is set up. Settings also mixes the CV profile, secrets and UI preferences on one scroll.

## Goal

One place for everything about the AI features, organized by role. Each role picks a provider, then a model from that provider, and shows whether it is ready. Frontend only: no IPC, store or backend changes.

## Navigation

Sidebar:

```
Jobs      (All jobs · + New job · Exports)
Library   (Experience · Projects · Education · Volunteer · Skills)
Profile
AI        (Roles · Models · Providers)
Settings
```

Routes:

| Route | Content |
|---|---|
| `/profile` | Profile form + Save (moved from Settings, unchanged behavior) |
| `/ai` | redirects to `/ai/roles` |
| `/ai/roles` | Role cards (default AI page) |
| `/ai/models` | Local model library: download, pause/resume, RAM fit |
| `/ai/providers` | OpenRouter key, Exa key, Llama.cpp status card |
| `/settings` | Preferences only: default page budget, theme |
| `/models` | redirects: `engine` tab → `/ai/roles`, any other tab → `/ai/models` |

The engine footer links to `/ai/roles`. Every "add a key" hint links to `/ai/providers`; every "download a model" hint links to `/ai/models`.

## Role cards (`/ai/roles`)

Two cards: **System One** (scores your library against each job) and **Company research** (researches each company on the web).

Each card:

- **Status badge:** Ready / Needs setup / Error.
- **Requirements row** (research only): "Exa key ✓" or "Add your Exa key →".
- **Provider toggle:** segmented control `Llama.cpp (local) | OpenRouter (online)`. It starts on the provider of the active model (Llama.cpp when none). Switching segments only changes which list is shown; the active model changes only when a model is picked.
- **Model list:** radio list for the selected provider, filtered by role, active one marked.

| Role | Llama.cpp models | OpenRouter models |
|---|---|---|
| System One | installed `decision` models | `listRemoteModels(false)` |
| Company research | installed `chat` models | `listRemoteModels(false, 'chat')` |

Llama.cpp entries that are not installed are listed with a "Download" link to `/ai/models`. Installed entries show size and RAM fit.

Segment states:

- **OpenRouter, no key:** list replaced by "Add your OpenRouter key →" (`/ai/providers`).
- **OpenRouter, key saved:** list with price and a Refresh button. The first pick shows the existing data-sharing notice (`ui.remote_notice_ack`) before activating.
- **Llama.cpp, nothing installed for the role:** "No model installed. Download one →".

Selecting a model calls the existing commands: System One → `setActiveModel` (via `ModelsStore.activate`) then `EngineStore.refresh`; research → `setResearchModel` (via `ModelsStore.useForResearch`).

**System One runtime line:** "llama.cpp on 127.0.0.1, started on the first match" or "OpenRouter (online)", engine state label, Restart button (re-activates the current model, as the old Engine tab did), and the engine error log in a collapsible block when present.

## Providers (`/ai/providers`)

Three cards:

- **Llama.cpp (local):** status only. Runtime state, this computer's RAM, count of installed models with a link to `/ai/models`. No key.
- **OpenRouter:** the existing `provider-key` component and data-sharing hint.
- **Exa:** the existing `provider-key` component and hint.

## Models (`/ai/models`)

The local catalog: today's model cards (download, pause/resume, retry, RAM fit, Recommended badge, role badge, Active badge). The "Use this model" / "Use for research" buttons are removed; selection happens on Roles. A finished download still auto-selects when that role has no model. The OpenRouter list moves into the Roles picker; the Installed/Download/Online/Engine tabs are removed.

## Error handling

- A failed role change leaves the previous selection and shows an error toast.
- A failed OpenRouter list shows the error inline in the list with Retry; the Llama.cpp segment is unaffected.
- When the active model's provider has no key, the card shows Needs setup with the reason.
- An engine error turns the System One badge to Error and shows the log; the footer dot already reflects it.

## Code layout

- `features/profile/profile-page.component.ts` (new). `profile-form.component.ts` moves here.
- `features/settings/settings-page.component.ts` keeps only Preferences.
- `features/ai/` (new): `role-card.component.ts`, `role-options.ts` (pure option builders, from `model-roles.component.ts`), `roles-page.component.ts`, `providers-page.component.ts`, `local-provider.component.ts`, `models-page.component.ts`. `provider-key.component.ts`, `model-cards.component.ts` move here.
- Removed: `features/models/` (page, remote-models), `settings/model-roles.component.ts`.
- `shell/app-sidebar.component.ts`, `shell/engine-footer.component.ts`, `app.routes.ts` updated. Onboarding keeps using `model-cards` from its new path.

## Testing

- `role-options` unit tests per role and provider.
- Role card: segment states (no key, nothing installed, ready), selection calls the right command, failure keeps the previous selection, notice before first OpenRouter pick.
- Routes: `/models?tab=engine` → `/ai/roles`, `/models?tab=download` → `/ai/models`, `/ai` → `/ai/roles`.
- Sidebar and engine footer link targets.
- Existing profile, settings, models and onboarding specs moved to new paths and kept green.

## Out of scope

Bring-your-own local server (Ollama, LM Studio), deleting downloaded models, any backend change.
