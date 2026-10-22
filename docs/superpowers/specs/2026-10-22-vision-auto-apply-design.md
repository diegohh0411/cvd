# Vision auto-apply design

Date: 2026-10-22. Builds on 2026-10-19-universal-autofill-design.md.

## Goal

When the active Fast Model can see images, it drives the apply window itself:
it looks at a screenshot of the page, navigates (finds the "Apply" button,
moves through multi-page forms) and fills fields. A human always submits.
Models without vision keep today's DOM-only autofill unchanged.

## Decisions

- Vision detection: provider metadata first (`architecture.input_modalities`
  from OpenRouter and llama.cpp servers), a probe otherwise (TypeSafe, SubQ).
  The probe sends a generated seven-segment digit image as a `choice`
  question over 0-9, twice with different digits. Both right with
  confidence >= 0.5 means vision. The result is cached per model id in
  settings (`vision:<model id>`). A 400/422 to an image request means no vision.
- Images go on the wire as System One `assets`
  (`{"<placeholder>": {"type": "image", "format": "png", "data_base64": ...}}`),
  with the placeholder inside `state`, and the header
  `TypeSafe-Danger-Zone-Images: true`.
- Screenshots: cross-platform window capture (`xcap`), cropped to the
  webview, scaled to at most 1280 px wide, PNG.
- Marks (set-of-marks): the injected script numbers visible interactive
  elements (buttons, links, fields) in every frame and draws a badge on each.
  The model picks `click:N` / `fill:N` instead of coordinates. Frame `n`
  numbers from `n * 100 + 1`, the top frame from 1.
- Clicks, scrolls and fills happen inside the page with DOM events, never with
  the OS mouse.
- Each step is one `choice` question. State: screenshot, URL, title, the
  marks (number, kind, label), profile fact names, the last 5 actions, and
  options skipped on this screen with the reason "low confidence". Options:
  `click:N`, `fill:N`, `scroll_down`, `scroll_up`, `back`, `done`, `ask_user`.
- Field values never come from the model: `fill:N` runs the existing `plan()`
  on that field (rules, memory, Fast Model pick between profile candidates).
  No value means the field is marked "needs you" and the step counts as a skip.
- Thresholds on the chosen option's probability:
  - `>= ACT_THRESHOLD` (0.6): run it.
  - between: skip. Drop it for this screen, tell the model, re-ask.
  - `< STOP_THRESHOLD` (0.25): stop the run.
  - At most `MAX_REASKS` (3) re-asks per screen; `MAX_STEPS` (40) per run.
- Submission is the human's. Two guards:
  - A: marks classified as submit (`type=submit`, or text like "Submit",
    "Send application", "Submit application") are never offered.
  - B: while the agent acts, `submit` events are cancelled and the run ends
    with "Ready for you to review and submit".
- Locking: only the apply window. A full-window transparent layer swallows
  pointer and keyboard input in every frame and shows a small banner
  ("cvd is filling this form…") with a Stop button. Every ending unlocks.

## Components

`src-tauri/src/autofill/agent/`:

- `vision.rs`: `supports_images(st, model)`; metadata lookup, probe, cache,
  digit PNG generator.
- `capture.rs`: `xcap` window capture, crop, scale, PNG base64.
- `step.rs`: pure. Builds the step question from marks, facts, history and
  skips; filters submit marks; maps the answer to `Act`, `Skip` or `Stop`.
- `run.rs`: the loop and its per-window registry (running flag, pending page
  replies, stop flag). Talks to the page and the screen through a `Page`
  trait and to the model through `DecisionEngine`, so it runs on fakes in tests.

`inject.js` gains an agent section: `mark`, `act`, `scroll`, `back`, `lock`,
`unlock`, the submit guard, and replies to cvd as `agent_reply` messages.

`DecisionEngine` gains `decide_with_images` (default: unsupported);
`HttpSystemOneEngine` implements it.

## One step

1. Lock (idempotent; a new page after navigation is locked again).
2. Ask every frame for marks; wait for the page to settle (300 ms without
   DOM changes, 5 s cap). Child frames reply through the top frame, which
   prefixes their keys with `fr<n>|`.
3. Capture the window.
4. Build and ask the question; map the answer.
5. Act: click/scroll/back in the owning frame, or fill through `plan()`.
6. Record history and repeat.

Hosts are checked with the existing consent flow before marks are read and
before a fill reaches a frame's host.

## Errors

- No capture (permission missing or window not found): notice, then hand off
  to the DOM path for this page.
- Engine error after the existing retry: stop with the error text.
- Image request rejected: cache "no vision", hand off to the DOM path.
- A mark gone before acting: counts as a skip, re-mark.
- Page reply timeout (navigation): re-mark on the next step.
- Window closed: the run ends.
- Any exit, including panic: a guard unlocks the window.

## Testing

- `step.rs`: submit marks never offered; skips listed in state and removed
  from options; act/skip/stop at the boundaries; re-ask and step budgets.
- `vision.rs`: metadata parsing from fixtures; probe pass/fail with a fake
  engine; non-vision on rejected images; caching.
- `wire.rs`: `assets` only when images are present; text requests unchanged.
- `http.rs`: image requests carry the header.
- `run.rs`: scripted engine and fake page: Apply → fill → reach submit → stop
  "ready for you"; Stop button; stop threshold; unlock on every exit.
- Manual: `CVD_AGENT_DUMP` (debug builds) writes each step's screenshot,
  marks and answer.

## Out of scope

Login walls and CAPTCHAs (`ask_user`), multiple tabs, threshold settings UI.
