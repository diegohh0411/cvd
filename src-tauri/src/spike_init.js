(() => {
  if (window.__spikeLoaded || window.top !== window) return;
  window.__spikeLoaded = true;
  const send = (msg) =>
    window.__TAURI_INTERNALS__.invoke('autofill_bridge', { msg });
  const cap = (s) => (s || '').toString().trim().slice(0, 200);
  const labelOf = (el) => {
    if (el.labels && el.labels[0]) return cap(el.labels[0].textContent);
    const lb = el.getAttribute('aria-labelledby');
    if (lb) { const n = document.getElementById(lb.split(' ')[0]); if (n) return cap(n.textContent); }
    return cap(el.getAttribute('aria-label') || el.placeholder || el.name);
  };
  const extract = () => [...document.querySelectorAll('input,select,textarea')]
    .filter((el) => el.type !== 'hidden' && !el.disabled)
    .slice(0, 300)
    .map((el) => ({
      key: el.name || el.id,
      id: el.id,
      kind: el.tagName === 'SELECT' ? 'select' : el.tagName === 'TEXTAREA' ? 'textarea' : el.type || 'text',
      label: labelOf(el),
      required: el.required || el.getAttribute('aria-required') === 'true',
      options: el.tagName === 'SELECT' ? [...el.options].slice(0, 20).map((o) => cap(o.text)) : undefined,
    }));

  const byKey = (k) => document.getElementById(k) || document.querySelector(`[name="${k}"]`);

  window.__spikeFill = ({ key, value }) => {
    const el = byKey(key);
    if (!el) return send({ type: 'fill_result', ok: false, error: 'no such field ' + key });
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    el.focus();
    setter.call(el, value);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
    el.blur();
    // After a re-render the value must still be there if the framework registered it.
    setTimeout(() => send({
      type: 'fill_result', ok: el.value === value, afterRender: el.value,
      stillInDom: document.contains(el), invalid: el.getAttribute('aria-invalid'),
      reactKey: Object.keys(el).filter((k) => k.startsWith('__react')).slice(0, 2),
    }), 1000);
  };

  window.__spikeFile = ({ name, b64 }) => setTimeout(() => {
    const input = document.querySelector('input[type=file]');
    if (!input) return send({ type: 'file_result', ok: false, error: 'no file input' });
    const bytes = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    const file = new File([bytes], name, { type: 'application/pdf' });
    const dt = new DataTransfer();
    dt.items.add(file);
    input.files = dt.files;
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
    setTimeout(() => {
      const scope = input.closest('form') || document.body;
      send({
        type: 'file_result',
        assigned: input.files.length, assignedName: input.files[0] && input.files[0].name,
        filenameShownInPage: scope.innerText.includes(name),
        errorText: [...scope.querySelectorAll('[role=alert],[class*=error]')].map((e) => cap(e.textContent)).filter(Boolean).slice(0, 5),
      });
    }, 1500);
  }, 3000);

  const start = () => {
    let tries = 0;
    const t = setInterval(() => {
      const fields = extract();
      if (fields.length > 3 || ++tries > 40) {
        clearInterval(t);
        send({ type: 'fields', url: location.href, fields }).catch((e) => console.error('bridge', e));
      }
    }, 500);
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', start);
  else start();
})();
window.addEventListener('error', (e) => window.__TAURI_INTERNALS__.invoke('autofill_bridge', { msg: { type: 'page_error', message: String(e.message).slice(0, 300) } }).catch(() => {}));
window.__spikeSend = (m) => window.__TAURI_INTERNALS__.invoke('autofill_bridge', { msg: m });
