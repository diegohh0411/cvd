// Injected into cvd's apply windows (autofill/window.rs). Describes the form to
// cvd, applies the fills cvd sends back, and shows what still needs the user.
// It never clicks submit: the only clicks are on radio buttons and combobox
// options it is filling.
(() => {
  if (window.top !== window || window.__cvdAutofill) return;

  const send = (msg) => {
    try {
      return window.__TAURI_INTERNALS__.invoke('autofill_bridge', { msg }).catch(() => {});
    } catch {
      return Promise.resolve();
    }
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

  const isVisible = (el) => {
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    return cs.display !== 'none' && cs.visibility !== 'hidden' && r.width > 0 && r.height > 0;
  };

  const textWithoutControls = (node) => {
    const c = node.cloneNode(true);
    c.querySelectorAll('input,select,textarea,button,[aria-hidden="true"]').forEach((n) => n.remove());
    return cap(c.textContent, 300);
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

  const byIds = (ids) =>
    cap(
      (ids || '')
        .split(/\s+/)
        .map((id) => id && document.getElementById(id))
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
      byIds(el.getAttribute('aria-labelledby')) ||
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

  const comboOptions = (el) => {
    const list = document.getElementById(el.getAttribute('aria-controls') || el.getAttribute('aria-owns') || '');
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
    // intl-tel-input's country search box.
    if (/iti.*search|search.*iti/i.test((el.id || '') + ' ' + (el.className || ''))) return true;
    // Radios, checkboxes and file inputs are often visually hidden behind custom UI.
    if (!['radio', 'checkbox', 'file'].includes(t) && !isVisible(el)) return true;
    return false;
  };

  const hasValue = (el) => {
    const k = kindOf(el);
    if (k === 'file') return el.files && el.files.length > 0;
    if (k === 'checkbox') return el.checked;
    return el.value != null && String(el.value).trim() !== '';
  };

  const extract = () => {
    const out = [];
    const groups = new Map();
    for (const el of document.querySelectorAll('input,select,textarea')) {
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
        help: byIds(el.getAttribute('aria-describedby')),
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
        (group && (byIds(group.getAttribute('aria-labelledby')) || cap(group.getAttribute('aria-label')))) ||
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

  const elsFor = (key) => [...document.querySelectorAll(`[data-cvd-key="${CSS.escape(key)}"]`)];

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
    render();
    clearTimeout(statusTimer);
    statusTimer = setTimeout(reportStatus, 1000);
  };

  // Progress for cvd (shown in the main window later; logged in dev builds).
  let statusTimer;
  const reportStatus = () => {
    const all = [...results.values()];
    send({
      type: 'status',
      filled: all.filter((r) => r.status === 'filled').length,
      needs: all.filter((r) => r.status === 'needs').map((r) => ({ label: cap(r.label, 200), reason: cap(r.reason, 200) })).slice(0, 100),
    });
  };

  // The user taking over a field clears its highlight.
  const watching = new WeakSet();
  const watch = (el, key) => {
    if (watching.has(el)) return;
    watching.add(el);
    const onUser = (e) => {
      if (!e.isTrusted) return;
      const r = results.get(key);
      if (r && r.status === 'needs' && hasValue(el)) mark(key, 'user');
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
    if (el.isConnected && el.value === value && el.getAttribute('aria-invalid') !== 'true') mark(key, 'filled');
    else mark(key, 'needs', 'The page rejected this value');
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

  const setCombobox = async (el, key, value) => {
    el.focus();
    nativeSet(el, value);
    fire(el, 'input');
    await later(500);
    const opt = [...document.querySelectorAll('[role="option"]')].find((o) => cap(o.textContent, 200) === value);
    if (opt) {
      opt.click();
      mark(key, 'filled');
    } else {
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

  const apply = (actions) => {
    for (const a of Array.isArray(actions) ? actions : []) {
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
  const render = () => {
    if (!document.body) return;
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
        @media (prefers-color-scheme: dark){.box{background:#1c1c1e;color:#eee;border-color:#333}.head,.foot{border-color:#333}
          li:hover{background:#2a2a2c} li small,.foot{color:#aaa}}
      </style>
      <div class="box">
        <div class="head" data-toggle><b>cvd autofill</b>
          <span class="n ok">${filled} filled</span><span class="n warn">${needs.length} need you</span></div>
        ${
          collapsed
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
        const el = elsFor(li.dataset.key)[0];
        if (!el) return;
        el.scrollIntoView({ block: 'center', behavior: 'smooth' });
        el.focus({ preventScroll: true });
      };
    });
  };

  // ---- lifecycle ----

  const scan = () => {
    const fields = extract();
    if (fields.length) send({ type: 'fields', url: location.href, fields });
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
      debounce = setTimeout(scan, 700);
    }
  });

  const start = () => {
    observer.observe(document.documentElement, { childList: true, subtree: true });
    const t0 = Date.now();
    const poll = setInterval(() => {
      const count = document.querySelectorAll('input,select,textarea').length;
      const quiet = Date.now() - lastMutation >= 500;
      if ((count >= 3 && quiet) || Date.now() - t0 > 20000) {
        clearInterval(poll);
        started = true;
        scan();
      }
    }, 250);
  };

  window.__cvdAutofill = { apply };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();
