# Universal autofill

Extends [auto-apply](2026-10-17-auto-apply-design.md) from three ATS hosts to any HTTPS application page.

## Problem

`SUPPORTED_HOSTS` (Greenhouse, Lever, Ashby) gates both the Apply button and the bridge, and `capabilities/apply.json` only lets those hosts call `autofill_bridge`. Postings on Workday, SmartRecruiters, Teamtailor, company career sites, or a company page that embeds a Greenhouse iframe fall back to the system browser. The injected script also only sees the top document's light DOM, so iframes and web components (shadow DOM) stay invisible.

## Goals

1. Autofill opens on any `https://` posting URL.
2. The page only receives profile data after the user agrees, per host, in a native dialog. Known ATS hosts and the job's own host count as agreed.
3. Extraction sees fields in open shadow roots and in iframes (same- and cross-origin).
4. The offline eval reports results per host and covers more ATS styles.

Non-goals: multi-step wizard handling, per-ATS adapters, login/CAPTCHA (step 3–4 of the plan, later PRs). cvd still never submits.

## Design

### 1. Any HTTPS URL, with host consent

- `is_openable_url(url)`: `https` and a host. Replaces `is_supported_url` at `start_apply`. `is_known_ats(url)` keeps the old list, only to skip the consent prompt and to label the button.
- `apply.json` remote URLs become `https://*/*`. The capability still grants exactly `allow-autofill-bridge`.
- `Session.allowed: BTreeSet<String>` holds hosts that may receive fills, seeded with the job URL's host. `Session.denied` holds hosts the user refused.
- On a `fields` message from a host that is neither known, allowed nor denied, Rust shows a native `tauri-plugin-dialog` confirm: *"Fill this application on {host} with your cvd profile?"*. The page cannot trigger, draw or answer it. Yes adds the host to `allowed`; No adds it to `denied` and the overlay says autofill is off for this host. One prompt at a time per window; messages during the prompt are dropped (the page rescans after any change).
- Only `fields` messages fill. `rejected` retries only for already-allowed hosts; `user_edit` learning is skipped on denied hosts; `submitted` still marks the job applied from any host (it reveals nothing).

Threat model: the page is hostile by default. Before consent it only learns that cvd is present. After consent, the approved host receives the facts it asked for, which is what the user would type anyway. A redirect or link to another host asks again.

### 2. Frames

- The init script is registered with `initialization_script_for_all_frames`. In a child frame it does not call Tauri. It extracts and fills its own document, and talks to the top frame over `postMessage` with a `__cvd` envelope.
- The top frame tracks child frames by `event.source`, records the browser-supplied `event.origin`, prefixes child field keys with `fr<n>|`, and forwards `{ type: "fields", origin, fields }`. Actions coming back are split by prefix and posted to that frame with `targetOrigin` set to its recorded origin.
- Rust uses `origin`'s host (when present) instead of the window URL for the consent check, so an embedded Greenhouse form inside `acme.com` asks about `job-boards.greenhouse.io` (known → no prompt), and an ad iframe never gets data without its own prompt.

### 3. Shadow DOM and labels

- `deepAll(selector)` walks the document and every open shadow root; `elsFor`, `extract` and the settle poll use it. ID lookups (`aria-labelledby`, `aria-controls`) resolve against the element's own root node.
- `autocomplete` was already extracted and matched by `matcher::by_autocomplete`; no change.

### 4. Eval per host

- `run` groups scores by fixture host (from `url`) as well as by fixture, and prints a per-host table.
- New fixtures, all labelled `synthetic:` in `url` since they are hand-written in the style of each ATS: `workday-style`, `smartrecruiters-style`, `career-site-generic`. `eval_rules_only` keeps the zero-wrong gate across all fixtures.

## Testing

- Rust: `is_openable_url`, `is_known_ats`, consent state transitions (`host_decision`), eval gate.
- Angular: `canAutofill` accepts any https URL.
- Manual: a Greenhouse embed on a company site, a Workday posting, decline then reopen.
