# CV Decisions — Design Spec

**Date:** 2026-10-08
**Status:** Draft for review

## 1. Intent

A local-first desktop app where a single user maintains a library of their professional experiences and projects. Given a job description (JD), a local decision model ranks the user's real entries and bullets against the role, and the app renders a ready-to-submit, ATS-friendly CV PDF.

**Constraints (agreed):**
- Single user. Offline and private by default — the only network use is the one-time model download from Hugging Face.
- Truthful CVs: the MVP selects and orders the user's own content; it never generates or rewrites text.
- UI/UX is the priority. Stack: Tauri 2, Angular 22, ZardUI, bun.

**Success criteria:**
- Paste a JD → reviewed, exported 1-page PDF in under ~2 minutes on a typical laptop.
- The user can see *why* each item was selected (scores) and override any choice.
- The exported PDF is fully text-extractable (ATS-readable) and never exceeds the page budget.

## 2. Architecture

```
Angular 22 + ZardUI  (presentation only)
        │  typed Tauri invoke() wrappers + events
Rust core (src-tauri)
 ├─ store    SQLite (sqlx + migrations)
 ├─ models   hf-hub downloads GGUFs → app data dir; progress events
 ├─ engine   DecisionEngine trait → LlamaSidecar impl
 ├─ matcher  JD + library → decision questions → ranked selection
 └─ render   selection → Typst (embedded crate) → PDF + page fit
```

- The webview never touches the database or the model directly; all logic lives in Rust.
- **Inference:** llama.cpp `llama-server` bundled as a Tauri sidecar (`externalBin`, `tauri-plugin-shell`), using llama.cpp's native decision-model support. It binds to `127.0.0.1` on a random free port, starts lazily on the first match, is health-checked, and is killed on app exit.
- **`DecisionEngine` trait:** `decide(state, questions) -> answers`, where questions are `noul`, `choice` or `score`. `LlamaSidecar` is the only production implementation. `FakeDecisionEngine` is used in tests.
- **Models:** the default is **Clef-flash** (~9B, Qwen3.5-based; ~6 GB at Q4, needs ~8 GB RAM). Optional: Clef 27B (~16–20 GB RAM). Fallback: Kev-4B. The model registry maps each model to an HF repo id, filename and checksum. The app checks RAM before offering a model.
- **PDF:** Typst, embedded as a Rust crate. Templates are Typst files that take selection data as input.

## 3. Data model (SQLite)

```
profile         (id, name, headline, email, phone, location, links_json, summary)
entry           (id, kind[experience|project|education|volunteer], title, org, location,
                 start_date, end_date, description, sort_hint, archived)
bullet          (id, entry_id → entry, text, position)
skill           (id, name UNIQUE, category)
entry_skill     (entry_id, skill_id)
job_description (id, title, company, raw_text, created_at)
match_run       (id, jd_id, model_id, status[running|done|cancelled|failed], created_at)
match_score     (run_id, target_type[entry|bullet|skill|meta], target_id, question,
                 probabilities_json, score)
cv              (id, jd_id, run_id, template, page_budget, selection_json, pdf_path, created_at)
```

- Bullets are separate rows, so they can be ranked individually.
- Experiences and projects share one `entry` table, with a `kind` column.
- Raw probabilities are persisted per run, so the UI can explain choices and re-rendering costs nothing.
- `cv.selection_json` holds the final selection after the user's edits, so every export is reproducible.

## 4. Matching pipeline

1. **Read the JD.** State = the JD text. Batched calls (≤128 questions each):
   - a `noul` for each skill in the library: "Does this role require or value {skill}?"
   - a `choice` for seniority: junior / mid / senior / lead
   - a `choice` for role family
2. **Score entries.** State = the JD + one entry (title, org, description, bullets, skills):
   - a `score` on a 0–4 rubric: "How relevant is this to the role?"
   - a `noul`: "Does it show the seniority this role needs?"
3. **Score bullets** for the top-K entries only: one call per entry, with one `score` question per bullet.
4. **Fit the page** (see §5).
5. **Review:** the user checks or unchecks items and drags to reorder. Each change re-fits the page and updates the preview live. The result is saved to `selection_json`.
6. **Export** the PDF.

- Progress is streamed to the UI as events: Reading JD → Scoring entries (n/N) → Picking bullets → Fitting page.
- Cancelling keeps the partial scores, and a run can be resumed.
- Scores are cached per run.
- Expected time: about 1 + N + K calls, roughly 30–90 seconds for 20 entries (depends on hardware).

## 5. Page-budget fitting

The fit is measured, not estimated. Typst is compiled in-process, and the page count is read from the compiled document.

1. **Required base:** header, the top-scoring experience with its top bullets, and the skills line built from the step-1 matches (ordered by weight).
2. **Candidate queue:** sorted by score. Each whole entry comes with its top 2 bullets. Each additional bullet is its own candidate. Candidates below a score floor are excluded.
3. **Greedy fit:** add the next candidate and compile. If it still fits the budget, keep it. If not, drop it and try the next one, since smaller items may still fit.
4. **Tightening:** before dropping a candidate that nearly fits, try the template's bounded tightening steps (e.g. 10.5→10pt font, tighter section gaps).
5. **Overflow order:** the lowest-scoring bullet is removed before any whole entry. Every included entry keeps at least 1 bullet.
6. **Section order:** within each section, entries are ordered by date, not by score.

The UI shows the fill level, e.g. "1 page · 92% full". Manual edits rerun steps 3–5.

## 6. ATS-safe templates

- MVP template: **Classic ATS**.
  - Single column.
  - Standard headings: Experience, Projects, Education, Skills.
  - No tables or text boxes for core content, and no text inside images.
  - Embedded fonts.
- Page budget: 1 page (default) or 2 pages.

## 7. UI/UX

The shell is ZardUI **`sidebar-07`**: a sidebar that collapses to an icon rail, beside an inset content area.

**Sidebar**
- **Header:** the app logo and the profile name.
- **Workspace:**
  - Jobs (All jobs, + New job, Exports)
  - Library (Experience, Projects, Education, Skills)
  - Models (Installed, Download, Engine)
  - Settings
- **Recent jobs:** the last few jobs, plus "More".
- **Footer:** model and engine status, e.g. "● Clef-flash · ready".

**Job workspace** (the inset)
- **Header:** sidebar trigger, a breadcrumb (`Jobs › {company} — {title}`), and Re-match and Export PDF actions.
- **Row of 3 cards:**
  - Role signals: skill probabilities and seniority
  - Match summary: selected/total, model, duration
  - Page fit: fill meter, template picker, budget picker
- **Main panel:** the selection list (tabs: JD, Selection, Exports) beside the live Typst PDF preview.
  - Each entry and bullet has a score bar, a checkbox and a drag handle.
  - Hovering a score shows its rubric probability breakdown.
- **States:** New (paste the JD) → Matching (stepper, cancellable) → Review.

**Library:** the same shell. The breadcrumb reads `Library › {kind}`, the cards show counts, and the main panel is a list with an editor in a side sheet.
- Fields: title, org, dates, kind.
- Bullets: a reorderable list.
- Skills: a tag combobox that creates new skills as you type.

**First run** (3-step dialog):
1. Profile.
2. Model: RAM check and a Clef-flash recommendation. The download runs in the background with pause/resume and checksum verification.
3. Add the first entry.

## 8. Error handling

- **Sidecar fails to start or crashes:** restart it once automatically, then show an actionable error (with a log excerpt) and a Retry button.
- **Model missing or corrupt** (checksum mismatch): route to the Models › Download screen.
- **Download interrupted:** resume with an HTTP range request, then verify the checksum.
- **Match cancelled or failed:** the run is marked `cancelled` or `failed`, partial scores are kept, and a Resume action is offered.
- **Typst compile error** (should not happen with our own templates): surface the error and keep the last good preview.
- **Library too small** (e.g. 0 entries): the Match button is disabled and the UI points to the Library.

## 9. Testing

- **Store:** unit tests on in-memory SQLite, plus migration up-tests.
- **Matcher:** tests with a `FakeDecisionEngine` that returns scripted probabilities. They cover ranking, K-selection, the score floor, cancel and resume.
- **Render:** golden tests per template.
  - The page count stays within budget.
  - Text extracted from the PDF contains every selected bullet. This is the automated ATS check.
  - Fit-loop edge cases: overflow ordering and the minimum-bullet rule.
- **Sidecar integration** (opt-in, behind a feature flag): start the real llama-server with a small decision model and check the shape of the response.
- **Angular:** component tests for the selection list, the fill meter and the first-run dialog, with `invoke` mocked.

## 10. Phase 0 spike (before building)

1. Confirm a **Clef-flash GGUF** exists on HF, or convert one. If neither is possible, fall back to Kev-4B as the default.
2. Run it through a `llama-server` sidecar on macOS from a minimal Tauri command.
3. Score 2–3 real JDs against the user's real entries, and check the rankings are sensible and the latency is acceptable.
4. Confirm the exact decision-API request format that llama.cpp accepts.

The spike's output is a go/no-go decision plus the default model choice. The spike code is throwaway.

## 11. Out of scope (MVP)

LLM rewriting or summaries, CV/LinkedIn import, cover letters, multiple profiles, cloud sync, the candle/pure-Rust engine, and more than one template.
