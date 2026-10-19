// Injected into cvd's apply windows (autofill/window.rs). Describes the form to
// cvd, applies the fills cvd sends back, and shows what still needs the user.
// It never clicks submit: the only clicks are on radio buttons and combobox
// options it is filling.
//
// It runs in every frame. Only the top frame talks to cvd and draws the
// overlay; child frames (an embedded ATS form, often cross-origin) relay
// through it with postMessage. The top frame tags each child's messages with
// the browser-reported origin, so cvd can ask the user about that host.
(() => {
  if (window.__cvdAutofill) return;
  const isTop = window.top === window;
  const UP = 'cvd-up';
  const DOWN = 'cvd-down';

  const invoke = (msg) => {
    try {
      return window.__TAURI_INTERNALS__.invoke('autofill_bridge', { msg }).catch(() => {});
    } catch {
      return Promise.resolve();
    }
  };
  const send = (msg) => {
    if (isTop) return invoke(msg);
    try {
      window.top.postMessage({ __cvd: UP, msg }, '*');
    } catch {
      /* detached frame */
    }
    return Promise.resolve();
  };
  const cap = (s, n = 500) => (s == null ? '' : String(s)).replace(/\s+/g, ' ').trim().slice(0, n);
  const norm = (s) => cap(s).toLowerCase();

  const COLORS = { filled: '#16a34a', needs: '#d97706' };
  /** key → { status: 'filled' | 'needs' | 'user', label, reason } */
  const results = new Map();
  const seen = new Set();
  let counter = 0;
  let pageErrors = 0;
  window.addEventListener('error', () => pageErrors++);

  // ---- extraction ----

  const SKIP_TYPES = new Set(['hidden', 'submit', 'button', 'reset', 'image', 'search', 'password']);
  const DATE_TYPES = new Set(['date', 'month', 'week', 'datetime-local', 'time']);

  /** querySelectorAll across the document and every open shadow root. */
  const deepAll = (sel, root = document, out = []) => {
    out.push(...root.querySelectorAll(sel));
    for (const el of root.querySelectorAll('*')) if (el.shadowRoot) deepAll(sel, el.shadowRoot, out);
    return out;
  };

  /** getElementById in the element's own tree (document or shadow root). */
  const byId = (el, id) => {
    if (!id) return null;
    const root = el && el.getRootNode ? el.getRootNode() : document;
    return (root.getElementById && root.getElementById(id)) || document.getElementById(id);
  };

  const isVisible = (el) => {
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    return cs.display !== 'none' && cs.visibility !== 'hidden' && r.width > 0 && r.height > 0;
  };

  /** Visible text only: skips controls and hidden helper text ("No location found"). */
  const textWithoutControls = (node) => {
    const parts = [];
    const walker = document.createTreeWalker(node, NodeFilter.SHOW_TEXT);
    const hiddenCache = new Map();
    const hidden = (el) => {
      if (!el || el === node.parentElement) return false;
      if (hiddenCache.has(el)) return hiddenCache.get(el);
      const h =
        /^(INPUT|SELECT|TEXTAREA|BUTTON|SCRIPT|STYLE)$/.test(el.tagName) ||
        el.getAttribute('aria-hidden') === 'true' ||
        el.getClientRects().length === 0 ||
        hidden(el.parentElement);
      hiddenCache.set(el, h);
      return h;
    };
    for (let t = walker.nextNode(); t; t = walker.nextNode()) if (!hidden(t.parentElement)) parts.push(t.textContent);
    return cap(parts.join(' '), 300);
  };

  /** Closest preceding label-like element within a few ancestors. */
  const precedingText = (el, selector, depth, exclude) => {
    let anc = el.parentElement;
    for (let i = 0; anc && i < depth; i++, anc = anc.parentElement) {
      const texts = [...anc.querySelectorAll(selector)]
        .filter((n) => !n.contains(el) && n.compareDocumentPosition(el) & Node.DOCUMENT_POSITION_FOLLOWING && !n.querySelector('input,select,textarea'))
        .map(textWithoutControls)
        .filter((t) => t && !(exclude && exclude.test(t)));
      if (texts.length) return texts[texts.length - 1];
    }
    return '';
  };

  const byIds = (ids, el) =>
    cap(
      (ids || '')
        .split(/\s+/)
        .map((id) => byId(el, id))
        .filter(Boolean)
        .map(textWithoutControls)
        .join(' '),
    );

  const labelOf = (el) => {
    if (el.labels && el.labels.length) {
      const t = textWithoutControls(el.labels[0]);
      if (t) return t;
    }
    return (
      byIds(el.getAttribute('aria-labelledby'), el) ||
      cap(el.getAttribute('aria-label')) ||
      precedingText(el, 'label,legend,[class*="label" i]', 4)
    );
  };

  const GENERIC = /^(attach|upload|choose|browse|select|enter manually|drop)/i;

  const sectionOf = (el) => {
    const fs = el.closest('fieldset');
    const legend = fs && fs.querySelector('legend');
    if (legend) return textWithoutControls(legend);
    // Upload widgets label the input with a button word ("Attach"); the real
    // question ("Resume/CV") is the nearest label-like text before the widget.
    if (kindOf(el) === 'file') {
      const t = precedingText(el, 'label,legend,h1,h2,h3,h4,h5,h6,[class*="label" i],[id*="label" i]', 6, GENERIC);
      if (t) return t;
    }
    return precedingText(el, 'h1,h2,h3,h4,h5,h6', 8);
  };

  const stamp = (el, key) => {
    el.dataset.cvdKey = key || el.dataset.cvdKey || 'f' + ++counter;
    return el.dataset.cvdKey;
  };

  const isCombobox = (el) =>
    el.getAttribute('role') === 'combobox' ||
    el.getAttribute('aria-autocomplete') === 'list' ||
    el.getAttribute('aria-haspopup') === 'listbox';

  /** react-select (Greenhouse and many others) keeps its options and a
   *  `selectOption` callback in component props, readable without opening it. */
  const reactSelect = (el) => {
    const fk = Object.keys(el).find((k) => k.startsWith('__reactFiber$'));
    let f = fk && el[fk];
    for (let i = 0; f && i < 15; i++, f = f.return) {
      const p = f.memoizedProps;
      if (p && Array.isArray(p.options) && typeof p.selectOption === 'function') return p;
    }
    return null;
  };

  /** Flattens grouped react-select options; keeps only text labels. */
  const reactOptions = (p) =>
    p.options.flatMap((o) => (o && Array.isArray(o.options) ? o.options : [o])).filter((o) => o && typeof o.label === 'string');

  const comboOptions = (el) => {
    const rs = reactSelect(el);
    if (rs) return reactOptions(rs).map((o) => cap(o.label, 200)).slice(0, 300);
    const list = byId(el, el.getAttribute('aria-controls') || el.getAttribute('aria-owns'));
    return list ? [...list.querySelectorAll('[role="option"]')].map((o) => cap(o.textContent, 200)).filter(Boolean) : [];
  };

  const kindOf = (el) => {
    if (el.tagName === 'SELECT') return 'select';
    if (el.tagName === 'TEXTAREA') return 'textarea';
    const t = (el.getAttribute('type') || 'text').toLowerCase();
    if (DATE_TYPES.has(t)) return 'date';
    if (['email', 'tel', 'url', 'number', 'file', 'radio', 'checkbox'].includes(t)) return t;
    return isCombobox(el) ? 'combobox' : 'text';
  };

  const skip = (el) => {
    if (el.disabled || el.readOnly || el.closest('[aria-hidden="true"]') || el.closest('#cvd-autofill-host')) return true;
    const t = (el.getAttribute('type') || '').toLowerCase();
    if (SKIP_TYPES.has(t)) return true;
    // Never touch payment fields, in any frame.
    if (/^cc-|\bcc-/i.test(el.getAttribute('autocomplete') || '')) return true;
    // intl-tel-input's country search box.
    if (/iti.*search|search.*iti/i.test((el.id || '') + ' ' + (el.className || ''))) return true;
    // Radios, checkboxes and file inputs are often visually hidden behind custom UI.
    if (!['radio', 'checkbox', 'file'].includes(t) && !isVisible(el)) return true;
    return false;
  };

  const hasValue = (el) => {
    const rs = reactSelect(el);
    if (rs) {
      const v = rs.getValue ? rs.getValue() : rs.value;
      return Array.isArray(v) ? v.length > 0 : v != null;
    }
    const k = kindOf(el);
    if (k === 'file') return el.files && el.files.length > 0;
    if (k === 'checkbox') return el.checked;
    return el.value != null && String(el.value).trim() !== '';
  };

  const extract = () => {
    const out = [];
    const groups = new Map();
    for (const el of deepAll('input,select,textarea')) {
      if (skip(el)) continue;
      const kind = kindOf(el);
      if (kind === 'radio') {
        const gid = el.name || (el.closest('[role="radiogroup"],fieldset') || el.parentElement).id || 'anon';
        if (!groups.has(gid)) groups.set(gid, []);
        groups.get(gid).push(el);
        continue;
      }
      const key = stamp(el);
      if (seen.has(key)) continue;
      seen.add(key);
      if (hasValue(el)) continue;
      out.push({
        key,
        kind,
        label: labelOf(el),
        placeholder: cap(el.getAttribute('placeholder'), 200),
        autocomplete: cap(el.getAttribute('autocomplete'), 100),
        name_attr: cap(el.getAttribute('name'), 200),
        help: byIds(el.getAttribute('aria-describedby'), el),
        section: sectionOf(el),
        required: el.required || el.getAttribute('aria-required') === 'true',
        options:
          kind === 'select'
            ? [...el.options].filter((o) => !o.disabled).map((o) => cap(o.text, 200))
            : kind === 'combobox'
              ? comboOptions(el)
              : [],
      });
      watch(el, key);
    }
    for (const radios of groups.values()) {
      const key = stamp(radios[0], radios[0].dataset.cvdKey || 'g' + ++counter);
      radios.forEach((r) => stamp(r, key));
      if (seen.has(key)) continue;
      seen.add(key);
      if (radios.some((r) => r.checked)) continue;
      const group = radios[0].closest('[role="radiogroup"],fieldset');
      const groupLabel =
        (group && (byIds(group.getAttribute('aria-labelledby'), group) || cap(group.getAttribute('aria-label')))) ||
        (group && group.querySelector('legend') && textWithoutControls(group.querySelector('legend'))) ||
        precedingText(radios[0], 'label:not(:has(input)),legend,[class*="label" i]', 5);
      out.push({
        key,
        kind: 'radio',
        label: groupLabel,
        name_attr: cap(radios[0].name, 200),
        section: sectionOf(radios[0]),
        required: radios.some((r) => r.required),
        options: radios.map((r) => labelOf(r)),
      });
      radios.forEach((r) => watch(r, key));
    }
    return out.slice(0, 300);
  };

  // ---- filling ----

  const elsFor = (key) => deepAll(`[data-cvd-key="${CSS.escape(key)}"]`);

  const nativeSet = (el, value) => {
    const proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : el.tagName === 'SELECT' ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, value);
  };

  const fire = (el, ...types) => types.forEach((t) => el.dispatchEvent(new Event(t, { bubbles: true })));

  const outline = (key, color) => {
    const els = elsFor(key);
    if (!els.length) return;
    const target = els.length > 1 ? els[0].closest('[role="radiogroup"],fieldset') || els[0].parentElement : els[0];
    target.style.outline = color ? `2px solid ${color}` : '';
    target.style.outlineOffset = color ? '2px' : '';
  };

  const mark = (key, status, reason) => {
    const el = elsFor(key)[0];
    const prev = results.get(key);
    results.set(key, { status, reason: reason || '', label: (prev && prev.label) || (el && (labelOf(el) || el.getAttribute('name'))) || key });
    outline(key, COLORS[status]);
    if (!isTop) {
      const r = results.get(key);
      try {
        window.top.postMessage({ __cvd: UP, mark: { key, status, reason: cap(r.reason, 200), label: cap(r.label, 200) } }, '*');
      } catch {
        /* detached frame */
      }
      return;
    }
    render();
    clearTimeout(statusTimer);
    statusTimer = setTimeout(reportStatus, 1000);
  };

  // Progress for cvd (shown in the main window later; logged in dev builds).
  let statusTimer;
  const reportStatus = () => {
    if (!isTop) return;
    const all = [...results.values()];
    send({
      type: 'status',
      filled: all.filter((r) => r.status === 'filled').length,
      needs: all.filter((r) => r.status === 'needs').map((r) => ({ label: cap(r.label, 200), reason: cap(r.reason, 200) })).slice(0, 100),
    });
  };

  /** What the user sees as the field's answer: typed text, option text, radio label. */
  const answerOf = (el, key) => {
    const rs = reactSelect(el);
    if (rs) {
      const v = rs.getValue ? rs.getValue() : [];
      const first = Array.isArray(v) ? v[0] : v;
      return first && typeof first.label === 'string' ? first.label : '';
    }
    const k = kindOf(el);
    if (k === 'radio') {
      const checked = elsFor(key).find((r) => r.checked);
      return checked ? labelOf(checked) : '';
    }
    if (k === 'select') return el.selectedIndex >= 0 ? cap(el.options[el.selectedIndex].text, 200) : '';
    if (k === 'file' || k === 'checkbox') return '';
    return cap(el.value, 500);
  };

  // The user taking over a field clears its highlight, and their answer is
  // sent to cvd so it can learn which profile detail this label means.
  const watching = new WeakSet();
  const editTimers = new Map();
  const watch = (el, key) => {
    if (watching.has(el)) return;
    watching.add(el);
    const onUser = (e) => {
      if (!e.isTrusted) return;
      const r = results.get(key);
      if (r && r.status === 'needs' && hasValue(el)) mark(key, 'user');
      clearTimeout(editTimers.get(key));
      editTimers.set(
        key,
        setTimeout(() => {
          // Child frames keep what the user types to themselves: relaying it
          // would show a cross-origin frame's input to the parent page.
          const value = isTop ? answerOf(el, key) : '';
          if (value) send({ type: 'user_edit', key, value });
        }, 800),
      );
    };
    el.addEventListener('input', onUser);
    el.addEventListener('change', onUser);
  };

  const later = (ms) => new Promise((r) => setTimeout(r, ms));

  const setText = async (el, key, value) => {
    el.focus();
    nativeSet(el, value);
    fire(el, 'input', 'change');
    el.blur();
    await later(600);
    if (el.isConnected && el.value === value && el.getAttribute('aria-invalid') !== 'true') return mark(key, 'filled');
    if (el.isConnected) {
      nativeSet(el, '');
      fire(el, 'input', 'change');
    }
    mark(key, 'needs', 'The page rejected this value');
    // cvd may answer with another candidate.
    send({ type: 'rejected', key, value });
  };

  const setSelect = (el, key, value) => {
    const opt = [...el.options].find((o) => cap(o.text, 200) === value);
    if (!opt) return mark(key, 'needs', 'Option not found');
    nativeSet(el, opt.value);
    fire(el, 'input', 'change');
    mark(key, 'filled');
  };

  const setRadio = (els, key, value) => {
    const r = els.find((x) => labelOf(x) === value);
    if (!r) return mark(key, 'needs', 'Option not found');
    r.click();
    mark(key, r.checked ? 'filled' : 'needs', r.checked ? '' : 'Could not select it');
  };

  const findOption = (el, value) => optionEls(el).find((o) => cap(o.textContent, 200) === value);

  const setCombobox = async (el, key, value) => {
    const rs = reactSelect(el);
    if (rs) {
      const o = reactOptions(rs).find((x) => cap(x.label, 200) === value);
      if (!o) return mark(key, 'needs', 'Option not found');
      rs.selectOption(o);
      await later(300);
      return mark(key, 'filled');
    }
    // Open and pick, as a user would; type to filter only if the option isn't rendered.
    await openCombobox(el);
    let opt = findOption(el, value);
    if (!opt) {
      nativeSet(el, value);
      fire(el, 'input');
      await later(500);
      opt = findOption(el, value);
    }
    if (opt) {
      opt.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      opt.click();
      await later(300);
      mark(key, 'filled');
    } else {
      closeCombobox(el);
      mark(key, 'needs', 'Pick from the list');
    }
  };

  // Uploaders hydrate late; an early assignment can throw inside the page. So
  // wait, assign, check the filename shows up, and retry twice before giving up.
  const attachFile = async (el, key, name, b64) => {
    await later(2500);
    for (let attempt = 0; attempt < 3; attempt++) {
      const input = elsFor(key)[0] || el;
      if (!input.isConnected) break;
      const errorsBefore = pageErrors;
      try {
        const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
        const dt = new DataTransfer();
        dt.items.add(new File([bytes], name, { type: 'application/pdf' }));
        input.files = dt.files;
        fire(input, 'input', 'change');
      } catch {
        /* retried below */
      }
      await later(1500);
      const scope = input.closest('form') || document.body;
      if (pageErrors === errorsBefore && scope.innerText.includes(name)) return mark(key, 'filled');
      await later(2000);
    }
    mark(key, 'needs', 'Attach your CV yourself');
  };

  // ---- frames (top frame only) ----

  /** Child frame window → { id, origin }. Keys from frame n are `fr<n>|key`. */
  const frames = new Map();
  let frameCounter = 0;
  const FRAME_KEY = /^fr(\d+)\|(.*)$/s;
  const frameById = (id) => [...frames.entries()].find(([, f]) => f.id === id);
  const prefixed = (f, key) => `fr${f.id}|${key}`;

  const postDown = (id, payload) => {
    const hit = frameById(id);
    if (!hit) return;
    try {
      hit[0].postMessage({ __cvd: DOWN, ...payload }, hit[1].origin);
    } catch {
      frames.delete(hit[0]);
    }
  };

  const onChildMessage = (e) => {
    const d = e.data;
    if (!d || d.__cvd !== UP || !e.source || e.source === window) return;
    if (!/^https:\/\//.test(e.origin)) return; // sandboxed or insecure frames get nothing
    let f = frames.get(e.source);
    if (!f) frames.set(e.source, (f = { id: ++frameCounter, origin: e.origin }));
    if (d.mark && typeof d.mark.key === 'string') {
      const k = prefixed(f, d.mark.key);
      results.set(k, { status: d.mark.status, reason: cap(d.mark.reason, 200), label: cap(d.mark.label, 200) || 'Embedded field' });
      render();
      clearTimeout(statusTimer);
      statusTimer = setTimeout(reportStatus, 1000);
      return;
    }
    const m = d.msg;
    if (!m || typeof m.type !== 'string') return;
    const msg = { ...m, origin: e.origin };
    if (Array.isArray(m.fields)) msg.fields = m.fields.slice(0, 300).map((x) => ({ ...x, key: prefixed(f, cap(x && x.key, 200)) }));
    if (typeof m.key === 'string') msg.key = prefixed(f, m.key);
    if (msg.type === 'status') return; // the top frame reports for everyone
    invoke(msg);
  };

  const onParentMessage = (e) => {
    const d = e.data;
    if (e.source !== window.top || !d || d.__cvd !== DOWN) return;
    if (d.kind === 'apply') apply(d.actions);
    else if (d.kind === 'focus') focusKey(d.key);
  };

  const focusKey = (key) => {
    const el = elsFor(key)[0];
    if (!el) return;
    el.scrollIntoView({ block: 'center', behavior: 'smooth' });
    el.focus({ preventScroll: true });
  };

  const apply = (actions) => {
    const list = Array.isArray(actions) ? actions : [];
    if (isTop) {
      const byFrame = new Map();
      for (const a of list) {
        const m = a && typeof a.key === 'string' && a.key.match(FRAME_KEY);
        if (!m) continue;
        const id = Number(m[1]);
        if (!byFrame.has(id)) byFrame.set(id, []);
        byFrame.get(id).push({ ...a, key: m[2] });
      }
      byFrame.forEach((acts, id) => postDown(id, { kind: 'apply', actions: acts }));
    }
    for (const a of list) {
      if (isTop && a && typeof a.key === 'string' && FRAME_KEY.test(a.key)) continue;
      const els = elsFor(a.key);
      if (!els.length) continue;
      const el = els[0];
      if (a.action === 'needs_user') {
        mark(a.key, 'needs', a.reason);
      } else if (a.action === 'file') {
        attachFile(el, a.key, a.name, a.b64);
      } else if (a.action === 'set') {
        if (hasValue(el) && kindOf(el) !== 'radio') continue; // the user got there first
        const kind = kindOf(el);
        if (kind === 'radio') setRadio(els, a.key, a.value);
        else if (kind === 'select') setSelect(el, a.key, a.value);
        else if (kind === 'combobox') setCombobox(el, a.key, a.value);
        else setText(el, a.key, a.value);
      }
    }
  };

  // ---- overlay ----

  let host, root, collapsed = false;
  let noticeText = '';
  const notice = (text) => {
    noticeText = cap(text, 300);
    render();
  };
  const render = () => {
    if (!isTop || !document.body) return;
    if (!results.size && !noticeText) return;
    if (!host) {
      host = document.createElement('div');
      host.id = 'cvd-autofill-host';
      host.style.cssText = 'position:fixed;right:16px;bottom:16px;z-index:2147483647;';
      root = host.attachShadow({ mode: 'closed' });
      document.body.appendChild(host);
    }
    if (!host.isConnected) document.body.appendChild(host);
    const all = [...results.entries()];
    const filled = all.filter(([, r]) => r.status === 'filled').length;
    const needs = all.filter(([, r]) => r.status === 'needs');
    const esc = (s) => cap(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
    root.innerHTML = `
      <style>
        .box{font:13px/1.4 -apple-system,system-ui,sans-serif;color:#111;background:#fff;border:1px solid #ddd;border-radius:10px;
             box-shadow:0 6px 24px rgba(0,0,0,.15);width:300px;max-height:60vh;display:flex;flex-direction:column;overflow:hidden}
        .head{display:flex;align-items:center;gap:8px;padding:10px 12px;border-bottom:1px solid #eee;cursor:pointer}
        .head b{flex:1} .n{font-variant-numeric:tabular-nums}
        .ok{color:${COLORS.filled}} .warn{color:${COLORS.needs}}
        ul{list-style:none;margin:0;padding:4px 0;overflow:auto}
        li{padding:6px 12px;cursor:pointer} li:hover{background:#f6f6f6}
        li small{display:block;color:#666}
        .foot{padding:8px 12px;border-top:1px solid #eee;color:#555;font-size:12px}
        .note{padding:8px 12px;background:#ecfdf5;color:#065f46;border-bottom:1px solid #d1fae5}
        @media (prefers-color-scheme: dark){.box{background:#1c1c1e;color:#eee;border-color:#333}.head,.foot{border-color:#333}
          .note{background:#064e3b;color:#d1fae5;border-color:#065f46}
          li:hover{background:#2a2a2c} li small,.foot{color:#aaa}}
      </style>
      <div class="box">
        <div class="head" data-toggle><b>cvd autofill</b>
          <span class="n ok">${filled} filled</span><span class="n warn">${needs.length} need you</span></div>
        ${noticeText ? `<div class="note">${esc(noticeText)}</div>` : ''}
        ${
          collapsed || noticeText
            ? ''
            : `<ul>${needs.map(([k, r]) => `<li data-key="${esc(k)}">${esc(r.label) || 'Unlabelled field'}<small>${esc(r.reason)}</small></li>`).join('')}</ul>
               <div class="foot">Review every answer, then submit it yourself.</div>`
        }
      </div>`;
    root.querySelector('[data-toggle]').onclick = () => {
      collapsed = !collapsed;
      render();
    };
    root.querySelectorAll('li[data-key]').forEach((li) => {
      li.onclick = () => {
        const m = li.dataset.key.match(FRAME_KEY);
        if (m) postDown(Number(m[1]), { kind: 'focus', key: m[2] });
        else focusKey(li.dataset.key);
      };
    });
  };

  // ---- lifecycle ----

  const press = (el, key) =>
    el.dispatchEvent(new KeyboardEvent('keydown', { key, code: key, keyCode: key === 'ArrowDown' ? 40 : 27, bubbles: true }));

  /** The open listbox that belongs to `el`: its aria-controls target, or a visible one beside it. */
  const listboxFor = (el) => {
    const own = byId(el, el.getAttribute('aria-controls') || el.getAttribute('aria-owns'));
    if (own) return own;
    let anc = el.parentElement;
    for (let i = 0; anc && i < 5; i++, anc = anc.parentElement) {
      const list = [...anc.querySelectorAll('[role="listbox"]')].find((l) => l.getClientRects().length > 0);
      if (list) return list;
    }
    return null;
  };

  const optionEls = (el) => {
    const list = listboxFor(el);
    return list ? [...list.querySelectorAll('[role="option"]')] : [];
  };

  const listOptions = (el) => optionEls(el).map((o) => cap(o.textContent, 200)).filter(Boolean);

  /** Opens a combobox (keyboard ArrowDown, like a user would) and returns its option elements. */
  const openCombobox = async (el) => {
    el.focus();
    press(el, 'ArrowDown');
    await later(200);
    if (!listOptions(el).length) {
      el.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
      await later(200);
    }
  };

  const closeCombobox = (el) => {
    press(el, 'Escape');
    el.blur();
  };

  // Comboboxes only render their options when open, so read them by opening
  // each one briefly. Nothing is selected.
  const probeOptions = async (fields) => {
    for (const f of fields) {
      if (f.kind !== 'combobox' || f.options.length) continue;
      const el = elsFor(f.key)[0];
      if (!el) continue;
      try {
        await openCombobox(el);
        f.options = listOptions(el).slice(0, 300);
      } finally {
        closeCombobox(el);
      }
      await later(50);
    }
  };

  let scanning = false;
  const scan = async () => {
    if (scanning) return;
    scanning = true;
    try {
      const fields = extract();
      await probeOptions(fields);
      if (fields.length) send({ type: 'fields', url: location.href, fields });
    } finally {
      scanning = false;
    }
  };

  // Wait for the form to settle: at least 3 controls and no DOM changes for 500 ms
  // (or 20 s, whichever first), then rescan on later changes (conditional questions,
  // multi-step forms).
  let lastMutation = Date.now();
  let debounce;
  let started = false;
  const observer = new MutationObserver((records) => {
    if (records.every((r) => host && host.contains(r.target))) return;
    lastMutation = Date.now();
    if (started) {
      clearTimeout(debounce);
      debounce = setTimeout(() => {
        checkConfirmation();
        scan();
      }, 700);
    }
  });

  const start = () => {
    checkConfirmation();
    observer.observe(document.documentElement, { childList: true, subtree: true });
    const t0 = Date.now();
    const poll = setInterval(() => {
      const count = deepAll('input,select,textarea').length;
      const quiet = Date.now() - lastMutation >= 500;
      if ((count >= 3 && quiet) || Date.now() - t0 > 20000) {
        clearInterval(poll);
        started = true;
        scan();
      }
    }, 250);
  };

  // ---- submit confirmation ----
  // cvd never submits. When the user does, and the page then shows a
  // confirmation, cvd marks the job as applied. sessionStorage carries the
  // submit across the navigation to the confirmation page (same origin).
  const SUBMIT_KEY = 'cvd-submit-at';
  const DONE_KEY = 'cvd-confirmed';
  const SUBMIT_WORDS = /\b(submit|apply|send)\b/i;
  const CONFIRM_TEXT =
    /thank you for (applying|your application|submitting)|thanks for applying|application (was |has been )?(received|submitted)|we('ve| have) received your application|successfully (submitted|applied)/i;
  const CONFIRM_URL = /\/(confirmation|thanks|thank-you|thank_you|applied)(\b|\/|$)|\/application\/(submitted|success)/i;

  const store = {
    get: (k) => {
      try {
        return sessionStorage.getItem(k);
      } catch {
        return null;
      }
    },
    set: (k, v) => {
      try {
        sessionStorage.setItem(k, v);
      } catch {
        /* storage blocked: detection only works on the same page */
      }
    },
  };
  let submitAtMemory = 0;

  const noteSubmit = () => {
    submitAtMemory = Date.now();
    store.set(SUBMIT_KEY, String(submitAtMemory));
    // Some forms swap in the confirmation without navigating.
    for (let i = 1; i <= 30; i++) setTimeout(checkConfirmation, i * 1000);
  };

  document.addEventListener(
    'click',
    (e) => {
      if (!e.isTrusted) return;
      const b = e.target.closest && e.target.closest('button,input[type="submit"],[role="button"],a');
      if (!b || b.closest('#cvd-autofill-host')) return;
      const text = cap(b.textContent || b.value || b.getAttribute('aria-label'), 100);
      if (b.type === 'submit' || SUBMIT_WORDS.test(text)) noteSubmit();
    },
    true,
  );
  document.addEventListener('submit', (e) => e.isTrusted && noteSubmit(), true);

  const checkConfirmation = () => {
    if (store.get(DONE_KEY)) return;
    const at = Math.max(Number(store.get(SUBMIT_KEY)) || 0, submitAtMemory);
    if (!at || Date.now() - at > 15 * 60 * 1000) return;
    const text = document.body ? document.body.innerText.slice(0, 20000) : '';
    if (!CONFIRM_TEXT.test(text) && !CONFIRM_URL.test(location.pathname)) return;
    store.set(DONE_KEY, '1');
    send({ type: 'submitted', url: location.href });
  };

  window.addEventListener('message', isTop ? onChildMessage : onParentMessage);
  window.__cvdAutofill = { apply, notice };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();
