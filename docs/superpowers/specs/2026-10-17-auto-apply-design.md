# Auto-apply with the Fast Model

## Problem

Applying to a saved job means retyping the same profile data into every ATS form: name, email, phone, location, links, resume, plus the same handful of yes/no questions (work authorization, sponsorship, relocation). cvd already holds all of that, and it already has a decision model (the Fast Model, e.g. Jev) that is cheap enough to call per field.

## Goals

1. An **Apply** button on a job opens the posting's URL in a cvd browser window.
2. cvd fills as much of the form as it can, first with plain code, then with the Fast Model choosing among candidates.
3. Fields cvd cannot fill with confidence are left empty and highlighted.
4. The user reviews everything and clicks Submit. **cvd never submits.**
5. After a submit, the job moves to the "Applied" status.

Non-goals (v1): Workday and other multi-step wizards that require account creation, CAPTCHAs, login walls, generating free text (cover letters, "Why us?"), EEO and demographic questions, applying to several jobs in a batch.

## Core idea: the Fast Model chooses, it never writes

A decision model cannot generate text (`TypeSafeProvider::chat_endpoint` errors by design). That's a strength here: for every field, code builds a short list of **candidates** taken from the user's real data, and the model picks one or `NONE`.

```
Field: "Legal first name" (text, required)
Candidates:
  a) Diego
  b) Diego Hernández
  c) Hernández
  d) Monterrey
  e) NONE: leave for the user
```

The worst outcome is a wrong-but-real value or an empty field. The model can never invent a phone number. All accuracy lives in the harness: how candidates are built, filtered, asked and validated.

## What exists today (reuse, do not regress)

- `engine::DecisionEngine::decide(state, questions)` takes up to `MAX_QUESTIONS` (128) questions per request. `Question::Choice { instructions, options }` returns `Answer::Choice { choice, probabilities }`, so every pick comes with a confidence.
- `decide_checked` enforces the per-engine question cap. Providers already map 401/402/429 to `EngineError`.
- `store::types::Profile` has `name`, `headline`, `email`, `phone`, `location` (one free-text string), `links`, `summary`.
- `JobDescription` has `url` and `status_id`. Job statuses are user-editable (`upsert_job_status`).
- `render::compile` produces the CV PDF for a job (`Cv.pdf_path`).
- Capabilities: `src-tauri/capabilities/default.json` grants permissions to the `main` window only.

## Design

### 1. Apply window

- `start_apply(job_id)` opens a `WebviewWindow` labelled `apply-<job_id>` on `job.url`. It is a real browser window. The user can scroll, type, log in, solve a CAPTCHA, or close it at any time.
- No iframe: most ATSs send `X-Frame-Options`/`frame-ancestors`, and a cross-origin iframe can't be read anyway.
- An `initialization_script` (bundled from `src/autofill/`, plain TS → one IIFE) runs on every page load in that window. It extracts fields, applies fills and draws the overlay.
- A new capability `apply.json` targets windows `apply-*` with **remote URLs allowed** and grants exactly one custom command, `autofill_bridge`. The page origin is hostile by default: everything it sends is parsed as data, size-capped, and never used to build file paths or SQL.
- The overlay (shadow DOM, fixed bottom-right) shows progress ("Filled 14 · Needs you 3"), a list of the fields that need you (clicking one scrolls to it), and a **Re-run** button. The fields themselves get a coloured outline: green filled by code, blue filled by the model, amber needs you.

### 2. Field extraction (injected script)

For each visible `input`, `select`, `textarea`, radio group, checkbox group and ARIA combobox, emit a `FieldSpec`:

```ts
interface FieldSpec {
  key: string;              // stable per page: name || id || xpath hash
  kind: 'text' | 'email' | 'tel' | 'url' | 'number' | 'date' | 'textarea'
      | 'select' | 'radio' | 'checkbox' | 'combobox' | 'file';
  label: string;            // <label for>, aria-labelledby, wrapping label, nearest preceding text
  placeholder?: string;
  autocomplete?: string;
  nameAttr?: string;
  help?: string;            // aria-describedby / sibling helper text
  required: boolean;
  options?: string[];       // select / radio / checkbox / combobox (visible options only)
  current?: string;         // skip fields the user already filled
  section?: string;         // nearest fieldset legend / heading
}
```

Hidden, disabled, honeypot (`tabindex=-1` + offscreen) and already-filled fields are skipped. A `MutationObserver` re-extracts when the form changes (conditional questions appear after an answer), debounced at 500 ms.

### 3. Facts and candidates (Rust)

`autofill::facts` turns the profile into typed facts. Each fact has a stable key and one or more **variants**:

| Fact key | Source | Variants |
|---|---|---|
| `first_name`, `last_name`, `full_name` | `Profile.name` split, or overrides in Application defaults | with and without accents |
| `email` | `Profile.email` | — |
| `phone` | `Profile.phone` | E.164, national, digits only |
| `city`, `region`, `country`, `location_full` | Application defaults (structured), falling back to `Profile.location` | country name, ISO code |
| `linkedin`, `github`, `website` | `Profile.links`, classified by host | — |
| `current_title`, `current_company` | latest open `Entry` of kind experience | — |
| `years_experience` | sum of experience entries | integer, and as bucket text ("3-5 years") |
| `work_authorization`, `needs_sponsorship`, `willing_to_relocate`, `notice_period`, `earliest_start`, `salary_expectation` | Application defaults | yes/no or text as stored |

**Application defaults** is a new section on the Profile page, stored in one settings row (`apply.defaults`, JSON): structured location, name overrides, the yes/no answers above, notice period, earliest start. Each one is optional. A missing default means that field goes to the user.

### 4. The harness, per field

```
FieldSpec
  │
  ├─ 1. Hard rules ── EEO / gender / race / disability / veteran / salary* → NEEDS_USER
  │                   file inputs → resume path; textarea → NEEDS_USER (v1)
  │
  ├─ 2. Memory ────── label normalised → fact key learned before? → fill, done
  │
  ├─ 3. Deterministic ─ autocomplete attr / input type / exact label match → fill, done
  │
  ├─ 4. Candidates ── build list, filtered by kind:
  │                     tel → phone variants only; email → email; number → numeric facts;
  │                     select/radio → the page's own options; text → all text facts
  │                   always append NONE
  │
  ├─ 5. Ask ───────── Question::Choice, batched with every other open field
  │
  ├─ 6. Accept? ───── choice ≠ NONE and p ≥ threshold (default 0.6) → fill
  │                   otherwise → NEEDS_USER
  │
  └─ 7. Validate ──── after filling: checkValidity(), aria-invalid, new error text nearby
                      invalid → retry once without that candidate → else clear + NEEDS_USER
```

\* salary is a hard rule unless `salary_expectation` is set in Application defaults, then it goes through steps 4–7 like anything else.

**Batching.** One round sends every open field as a separate question in one `decide` call (chunked at the engine's `max_questions`). A typical 30-field form is one request.

**Per-question instructions** (JSON `instructions`, same pattern as matching):

```json
{
  "field": { "label": "Legal first name", "kind": "text", "required": true,
             "section": "Personal information", "placeholder": "", "help": "" },
  "already_filled": { "Email": "email", "Phone": "phone" },
  "question": "Which value belongs in `field`? Pick NONE if none fits exactly."
}
```

`already_filled` maps labels to fact keys (not values), so "Last name" knows "First name" took `first_name` and won't pick it again.

**Hierarchical fallback.** If the text candidate list is longer than 8, round 1 asks for a category (`identity | contact | location | links | experience | preferences | NONE`), and round 2 asks within it. Both rounds stay batched across fields.

**Selects with many options** (country lists with 250 entries): code narrows to options that fuzzy-match any fact variant (top 10). If nothing matches, the field goes to the user without calling the model.

### 5. Filling

- Text inputs: set the value through the native setter (`Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set`), then dispatch `input` and `change` so React/Vue-controlled forms register it.
- Selects and radios: select the matching option and dispatch `change`. Comboboxes: focus, type the value, wait for the listbox, click the matching option.
- File inputs (resume): Rust reads the job's latest `Cv.pdf_path` (or renders one) and sends the bytes as base64 through the bridge. The script builds a `File` and assigns it via `DataTransfer` to `input.files`. **Spike needed on WKWebView (macOS) and WebView2 (Windows).** If it fails, the field goes to the user with a "Reveal resume in Finder" link.
- The script never clicks a button whose text or type looks like submit, apply or send. The only clicks it makes are inside comboboxes it is filling.

### 6. Learning from the user

When the user changes a field cvd filled, or fills one cvd left empty with a value that equals a fact variant, cvd records `normalised label → fact key` in `apply_memory` (sqlite: `label_norm TEXT PRIMARY KEY, fact_key TEXT, hits INTEGER, updated_at TEXT`). Step 2 of the harness uses it next time on any site. A user edit to a value that matches no fact is not recorded.

### 7. Finishing

- The script watches for navigation or a DOM swap after a submit-like click by the user. If the new page matches confirmation patterns ("thank you for applying", "application received", ATS-specific URLs), it reports `submitted`.
- Rust moves the job to the status named "Applied" if one exists (case-insensitive), otherwise it does nothing and the overlay says so. A toast in the main window confirms.
- The Gmail job-updates scan can later confirm the same application from the receipt email. That's out of scope here, but it should key on the same `job_id`.

### 8. Bridge messages

Page → Rust (`autofill_bridge`, one command, tagged JSON):

| Message | Payload | Rust does |
|---|---|---|
| `fields` | `FieldSpec[]` (max 300, strings capped at 500 chars) | runs the harness, replies with `fill[]` |
| `filled` | `{ key, ok, error? }[]` | validation retries |
| `user_edit` | `{ key, label, value }` | memory update (value compared to facts in Rust, never stored raw) |
| `submitted` | `{ url }` | status change |

Rust → page: `window.eval` of `__cvdAutofill.apply(<json>)` with `{ key, action: 'set' | 'file' | 'needs_user', value? }[]`.

The page learns only the values for its own fields. It never gets the whole profile.

## Evaluation

The harness is the product, so it ships with an offline eval:

- `src-tauri/tests/fixtures/forms/*.html`: 20–30 saved forms (Greenhouse, Lever, Ashby, SmartRecruiters, a few custom ones), each with `*.expected.json` mapping `field key → fact key | NONE`.
- `cargo test autofill_eval -- --ignored` runs extraction (via a headless DOM, or extracted `FieldSpec` JSON committed next to each fixture) and the harness against the active Fast Model, then prints per-form and overall: **correct fill**, **wrong fill**, **abstain**, **abstain when a fill was possible**.
- Target before release: wrong fill ≤ 2 %, correct fill ≥ 75 % of fillable fields. A wrong fill costs more than an abstain, so the threshold is tuned on wrong fill first.
- Unit tests (not ignored) cover facts, candidate filtering, hard rules, memory and the accept/threshold logic with `engine::fake`.

## Cost

Jev bills $0.042 / Mtok input. A 30-field form is one or two batched requests at roughly 300–500 tokens per question, so about 10–30k tokens, around $0.001 per application.

## Rollout

1. **Spikes**: (a) remote-URL capability + `autofill_bridge` round trip from an injected script on a real Greenhouse page; (b) `DataTransfer` file assignment in WKWebView. Either failing changes §1 or §5.
2. Facts + Application defaults section + unit tests.
3. Extraction script + deterministic fill only (steps 1–3), no model. Usable on its own.
4. Model steps 4–7 + eval fixtures.
5. Memory, confirmation detection, status change.

## Open questions

- Should the threshold be a user setting, or fixed and tuned with the eval?
- Windows WebView2 parity: test there in the same spikes, or ship macOS first?
- Free-text questions: a later version could hand them to the Smart Model as drafts the user must approve. Not in v1.
