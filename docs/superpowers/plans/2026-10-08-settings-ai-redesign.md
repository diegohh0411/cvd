# Settings & AI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split Settings into Profile, Preferences and an AI section (Roles, Models, Providers) where each role picks a provider then a model.

**Architecture:** Frontend-only rearrangement. New `features/profile` and `features/ai` folders; `features/models` and `settings/model-roles` are removed. All IPC commands and stores (`ModelsStore`, `EngineStore`, `api.*`) are reused unchanged.

**Tech Stack:** Angular (standalone, signals, OnPush), Zard UI, Tailwind, Vitest via `bun --bun ng test`, Tauri IPC mocked with `testing/tauri-mock`.

**Spec:** `docs/superpowers/specs/2026-10-08-settings-ai-redesign-design.md`

## Global Constraints

- No changes under `src-tauri/`, `src/app/core/ipc/` or `src/app/core/stores/`.
- Provider toggle labels: `Llama.cpp (local)` and `OpenRouter (online)`.
- Role card titles: `System One`, `Company research`. Status badges: `Ready`, `Needs setup`, `Error`.
- Online model ids keep the `openrouter:<id>` form; local ids are the bare `spec.id`.
- The OpenRouter data-sharing notice is gated by `REMOTE_NOTICE_SETTING = 'ui.remote_notice_ack'` (value `'1'` once acknowledged).
- Every "add a key" link → `/ai/providers`; every "download a model" link → `/ai/models`.
- Tests run with `bun --bun ng test`; all existing specs stay green after each task.

## Review Focus

- Active System One model is online but the OpenRouter key was removed → card shows `Needs setup` and the OpenRouter segment shows the add-key link (Task 4 test `needs setup when key missing for active online model`).
- Research model points at a local model that is no longer installed → card shows `Needs setup`, not `Ready` (Task 4 test `needs setup when active local model is not installed`).
- Picking a model fails in the backend → radio returns to the previous model (Task 4 test `keeps previous selection on failure`).
- OpenRouter list request fails → inline error with Retry; Llama.cpp segment still lists local models (Task 4 test `shows inline error when online list fails`).
- Old deep links (`/models?tab=engine`, `/models?tab=download`, `/models` from readiness blockers) still land somewhere useful (Task 1 route tests, Task 6 readiness test).

---

### Task 1: Routes, redirects and Profile page

**Files:**
- Create: `src/app/features/profile/profile-page.component.ts`
- Move: `src/app/features/settings/profile-form.component.ts` → `src/app/features/profile/profile-form.component.ts`
- Modify: `src/app/features/settings/settings-page.component.ts` (Preferences only)
- Modify: `src/app/features/onboarding/first-run-dialog.component.ts:15` (import path)
- Modify: `src/app/app.routes.ts`
- Create: `src/app/app.routes.spec.ts`

**Interfaces:**
- Produces: `ProfilePageComponent` (selector `app-profile-page`); `modelsRedirect(tab: string | undefined): string` exported from `app.routes.ts`, returning `'/ai/roles'` for `'engine'`, else `'/ai/models'`.
- Produces routes: `profile`, `ai` (redirect → `ai/roles`), `ai/roles`, `ai/models`, `ai/providers`, `models` (redirect via `modelsRedirect`), `settings`. Until Tasks 3–5 create the real pages, `ai/roles`, `ai/models` and `ai/providers` load the existing `features/models/models-page.component` so the app builds; each later task repoints its own route.

- [ ] **Step 1: Write failing tests** in `app.routes.spec.ts`:

```ts
it('maps old models tabs', () => {
  expect(modelsRedirect('engine')).toBe('/ai/roles');
  expect(modelsRedirect('download')).toBe('/ai/models');
  expect(modelsRedirect(undefined)).toBe('/ai/models');
});
it('navigates /ai and /models to the new pages', async () => {
  // RouterTestingHarness with `routes`
  expect(await navigate('/ai')).toBe('/ai/roles');
  expect(await navigate('/models?tab=engine')).toBe('/ai/roles');
  expect(await navigate('/models?tab=online')).toBe('/ai/models');
});
```

And a profile-page test: renders the form with the stored profile, `Save profile` calls `save_profile`.

- [ ] **Step 2:** Run `bun --bun ng test` → new tests FAIL.
- [ ] **Step 3: Implement.** `/models` uses a `redirectTo` function reading `route.queryParams['tab']`. Profile page = the Profile `<section>` and `saveProfile()` logic lifted verbatim from `settings-page.component.ts`, under `<app-page-header [crumbs]="[{ label: 'Profile' }]" />`. Settings keeps budget + theme only, header `Preferences` with subtitle `How the app looks and behaves.`.
- [ ] **Step 4:** Run `bun --bun ng test` → PASS.
- [ ] **Step 5:** Commit `feat(nav): profile page, AI routes and models redirects`.

### Task 2: Sidebar, footer and blocker links

**Files:**
- Modify: `src/app/shell/app-sidebar.component.ts` (NAV + Profile link)
- Modify: `src/app/shell/engine-footer.component.ts` (link → `/ai/roles`, no query)
- Modify: `src/app/features/jobs/match-readiness.ts:27`, `src/app/features/jobs/new-job.component.ts:110`, `src/app/features/workspace/workspace-page.component.ts:330` (→ `/ai/models`, drop `tab` query)
- Modify: `src/app/features/jobs/jobs.spec.ts:26`, `src/app/shell/shell.spec.ts`

**Interfaces:**
- Produces NAV entry `{ title: 'AI', icon: 'lucideBot', base: '/ai', items: [Roles → /ai/roles, Models → /ai/models, Providers → /ai/providers] }` replacing `Models`; a flat `Profile` menu item (icon `lucideUserRound`, link `/profile`) after Library, before Settings.

- [ ] **Step 1: Update tests:** `jobs.spec.ts` expects `{ link: '/ai/models' }` with no `queryParams`; `shell.spec.ts` asserts the footer `href` ends with `/ai/roles` and the sidebar renders links to `/profile`, `/ai/roles`, `/ai/models`, `/ai/providers`.
- [ ] **Step 2:** Run tests → FAIL.
- [ ] **Step 3:** Implement link changes.
- [ ] **Step 4:** Run tests → PASS.
- [ ] **Step 5:** Commit `feat(nav): sidebar AI section and profile link`.

### Task 3: Providers page with Llama.cpp status card

**Files:**
- Move: `src/app/features/settings/provider-key.component.ts` → `src/app/features/ai/provider-key.component.ts` (copy unchanged)
- Create: `src/app/features/ai/local-provider.component.ts`
- Create: `src/app/features/ai/providers-page.component.ts`
- Create: `src/app/features/ai/ai.spec.ts`
- Modify: `src/app/app.routes.ts` (`ai/providers` → `ProvidersPageComponent`)

**Interfaces:**
- Produces: `LocalProviderComponent` (`app-local-provider`), `ProvidersPageComponent` (`app-providers-page`).

- [ ] **Step 1: Write failing tests** in `ai.spec.ts`: local card shows `Llama.cpp (local)`, `16 GB RAM`, `2 models installed` with a link to `/ai/models`, and the engine state label; providers page renders three cards in order Llama.cpp, OpenRouter, Exa.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3: Implement.** Local card reads `ModelsStore.ramGb()`, `ModelsStore.installed().length`, `EngineStore.stateLabel()`; calls `models.load()` and `engine.refresh()` in `ngOnInit`. Card frame: `rounded-lg border p-4 grid gap-2 text-sm`; wrap each `app-provider-key` in the same frame.
- [ ] **Step 4:** Run → PASS.
- [ ] **Step 5:** Commit `feat(ai): providers page with local inference card`.

### Task 4: Role options and role cards

**Files:**
- Create: `src/app/features/ai/role-options.ts`
- Create: `src/app/features/ai/role-card.component.ts`
- Create: `src/app/features/ai/roles-page.component.ts`
- Modify: `src/app/features/ai/ai.spec.ts`
- Modify: `src/app/features/companies/companies.spec.ts:10,76-90` (move role-option tests to `ai.spec.ts`)
- Modify: `src/app/app.routes.ts` (`ai/roles` → `RolesPageComponent`)
- Delete: `src/app/features/settings/model-roles.component.ts`

**Interfaces:**
- Produces in `role-options.ts`:
  - `type Role = 'system-one' | 'research'`
  - `type ProviderKind = 'local' | 'openrouter'`
  - `interface RoleOption { id: string; label: string; installed: boolean; detail: string }`
  - `providerOf(id: string | null): ProviderKind` → `'openrouter'` when id starts with `openrouter:`, else `'local'`.
  - `localOptions(role: Role, models: ModelInfo[]): RoleOption[]` — all models of kind `decision` (system-one) or `chat` (research), installed first; `detail` = `formatBytes(size_bytes)`.
  - `onlineOptions(lists: ProviderModels[]): RoleOption[]` — `id` = `openrouter:<m.id>`, `label` = `display_name`, `installed: true`, `detail` = `formatPrice(price_per_mtok_in)`.
  - `roleStatus(i: { active: string | null; installed: ModelInfo[]; hasOpenRouterKey: boolean; hasExaKey?: boolean; engineError?: boolean }): 'ready' | 'needs-setup' | 'error'`.
- Produces: `RoleCardComponent` (`app-role-card`, input `role: Role`); `RolesPageComponent` renders both cards.
- Moves `formatPrice` and `REMOTE_NOTICE_SETTING` from `remote-models.component.ts` into `role-options.ts` (Task 5 deletes the old file).

- [ ] **Step 1: Write failing unit tests** for `role-options`:

```ts
expect(providerOf('openrouter:a/b')).toBe('openrouter');
expect(providerOf('kev-4b')).toBe('local');
expect(localOptions('research', models).map((o) => o.id)).toEqual(['chat-installed', 'chat-missing']);
expect(localOptions('system-one', models).every((o) => o.id !== 'chat-installed')).toBe(true);
expect(onlineOptions(remote)[0]).toEqual({ id: 'openrouter:deepseek/x', label: 'DeepSeek X', installed: true, detail: '' });
expect(roleStatus({ active: null, installed: [], hasOpenRouterKey: false })).toBe('needs-setup');
expect(roleStatus({ active: 'openrouter:x', installed: [], hasOpenRouterKey: false })).toBe('needs-setup');
expect(roleStatus({ active: 'kev-4b', installed: [], hasOpenRouterKey: true })).toBe('needs-setup');
expect(roleStatus({ active: 'kev-4b', installed: [kev], hasOpenRouterKey: false, hasExaKey: false })).toBe('needs-setup'); // research
expect(roleStatus({ active: 'kev-4b', installed: [kev], hasOpenRouterKey: false, engineError: true })).toBe('error');
```

- [ ] **Step 2: Write failing component tests** (`ai.spec.ts`, `mockTauri`):
  - `starts on the active model's provider`: active `openrouter:x` → OpenRouter segment `aria-pressed="true"`.
  - `needs setup when key missing for active online model`: badge `Needs setup`, link `Add your OpenRouter key` → `/ai/providers`.
  - `needs setup when active local model is not installed`.
  - `lists not-installed local models with a Download link` → `/ai/models`; radio disabled.
  - `picking a local model calls set_active_model` (system-one) / `set_research_model` (research).
  - `asks for notice before first OpenRouter pick`: no `ui.remote_notice_ack` → notice shown, no `set_active_model` until `Use …` clicked; then `set_ui_setting` with `'1'`.
  - `keeps previous selection on failure`: `set_active_model` rejects → previously active radio still checked, error toast.
  - `shows inline error when online list fails`: `list_remote_models` returns `[{ provider: 'openrouter', models: [], error: 'boom' }]` → `boom` and `Retry`; switching to Llama.cpp still lists local models.
  - `research card shows Exa requirement`: no Exa key → `Add your Exa key` link.
  - `system one card shows runtime and restart`: Restart calls `set_active_model` with the active id.
- [ ] **Step 3:** Run → FAIL.
- [ ] **Step 4: Implement.** Card loads `listModels` (via `ModelsStore.load`), `providerKeyStatus('openrouter'|'exa')`, `getResearchModel` (research) and `listRemoteModels(false, role === 'research' ? 'chat' : 'decision')` only when the OpenRouter key exists. State: `active`, `viewing: ProviderKind`, `onlineLists`, `pending`. Provider toggle = two `<button aria-pressed>` in the same `bg-muted rounded-lg p-1` style as the old Models tabs. Model list = `<input type="radio" name="{role}-model">` rows; selection is optimistic, reverted on catch. System One footer: runtime text from `EngineStore.remote()`, `stateLabel()`, Restart, and `<details>` with `status().error`. Copy from spec (subtitles: `Scores your library against each job.` / `Researches each company on the web.`).
- [ ] **Step 5:** Run → PASS.
- [ ] **Step 6:** Commit `feat(ai): role cards with provider toggle`.

### Task 5: Models library page and cleanup

**Files:**
- Move: `src/app/features/models/model-cards.component.ts` → `src/app/features/ai/model-cards.component.ts`
- Create: `src/app/features/ai/models-page.component.ts`
- Delete: `src/app/features/models/` (all three files)
- Modify: `src/app/features/onboarding/first-run-dialog.component.ts:14` (import path)
- Modify: `src/app/app.routes.ts` (`ai/models` → new `ModelsPageComponent`)
- Modify: `src/app/features/ai/ai.spec.ts`

**Interfaces:**
- Consumes: none new. Produces: `ModelsPageComponent` (`app-ai-models-page`).

- [ ] **Step 1: Write failing tests:** models page lists all catalog models with RAM text from the old Download tab; cards show no `Use this model` / `Use for research` buttons; installed models still show `Active` when selected for a role; a finished download auto-selects when that role has none (existing `download()` behavior, assert `set_research_model` called for a chat model with no research model).
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3: Implement.** Remove the two "Use" buttons from `model-cards`; keep `autoActivate`. Page = header crumbs `[{ label: 'AI' }, { label: 'Models' }]`, the old Download-tab paragraph, `<app-model-cards />`. Update the onboarding import; confirm `first-run.spec.ts` still passes (onboarding relies on auto-activate, not the buttons).
- [ ] **Step 4:** Run `bun --bun ng test` and `bun --bun ng build` → PASS, no references to `features/models` or `features/settings/provider-key`.
- [ ] **Step 5:** Commit `feat(ai): models library page; remove old models page`.

### Task 6: Final pass

**Files:**
- Modify: any remaining copy pointing at "Settings" for keys (`grep -rn "in Settings\|Models page" src/app`).

- [ ] **Step 1:** Grep and replace copy with links to `/ai/providers` / `/ai/models`.
- [ ] **Step 2:** Run `bun --bun ng test` and `bun --bun ng build` → PASS.
- [ ] **Step 3:** Run the app (`run` skill), visit `/profile`, `/settings`, `/ai/roles`, `/ai/models`, `/ai/providers`, `/models?tab=engine`; screenshot Roles in light and dark.
- [ ] **Step 4:** Commit `chore(ai): update links and copy`.
