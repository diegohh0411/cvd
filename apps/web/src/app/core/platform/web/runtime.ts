import type { Transport } from '../transport';
import type { Platform } from '../platform';

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
let source: EventSource | null = null;

/** One `EventSource` for all listeners; frames are `{ event, payload }`. */
function connect(): void {
  if (source) return;
  source = new EventSource('/api/events');
  source.onmessage = (msg) => {
    const { event, payload } = JSON.parse(msg.data) as { event: string; payload: unknown };
    handlers.get(event)?.forEach((cb) => cb({ payload }));
  };
}

/** Talks to cvd-server: `POST /api/rpc/:command` and server-sent events on `/api/events`. */
export const transport: Transport = {
  async call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    const res = await fetch(`/api/rpc/${cmd}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(args ?? {}),
    });
    const body = await res.text();
    const value = body ? JSON.parse(body) : null;
    // Error bodies are the same `{ kind, message }` Tauri rejects with.
    if (!res.ok) throw value ?? new Error(`${cmd} failed: ${res.status}`);
    return value as T;
  },
  async listen<T>(event: string, cb: (event: { payload: T }) => void) {
    connect();
    const set = handlers.get(event) ?? new Set<Handler>();
    handlers.set(event, set);
    set.add(cb as Handler);
    return () => { set.delete(cb as Handler); };
  },
};
export const platform: Platform = {
  capabilities: { autoApply: false, updater: false, revealInFolder: false, localServer: false },
  async openUrl(url) { window.open(url, '_blank', 'noopener,noreferrer'); },
  async saveFile(bytes, suggestedName) {
    const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'application/pdf' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = suggestedName;
    document.body.append(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    return suggestedName;
  },
  async reveal() { throw new Error('Revealing local files is unavailable on web'); },
  async openFile() { throw new Error('Opening local files is unavailable on web'); },
  async version() { return 'web'; },
  async checkForUpdates() { return null; },
  async relaunch() { window.location.reload(); },
  async exportCv(runId, selection, suggestedName) {
    const cv = await transport.call<{ id: number; pdf: number[] }>('export_cv', { runId, selection });
    return platform.saveFile(new Uint8Array(cv.pdf), suggestedName);
  },
};
