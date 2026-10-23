import type { Transport } from '../transport';
import type { Platform } from '../platform';
import { accessToken, auth, syncSession } from './auth';

type Handler = (event: { payload: unknown }) => void;
const handlers = new Map<string, Set<Handler>>();
let source: EventSource | null = null;
let ready: Promise<void> | null = null;

function dispatch(event: string, payload: unknown): void {
  handlers.get(event)?.forEach((cb) => cb({ payload }));
}

/** A connection the browser gave up on (e.g. 401) is retried after this long. */
const RETRY_MS = 3000;

/**
 * One `EventSource` for all listeners; frames are `{ event, payload }`. Resolves once the
 * server has subscribed us (its `ready` frame), so callers can start work without losing events.
 * Events sent while disconnected are gone, so every reconnect refreshes all tables.
 */
function connect(): Promise<void> {
  if (ready) return ready;
  let opened = false;
  ready = new Promise((resolve) => {
    const open = async () => {
      // The stream authenticates by cookie, so it must hold the restored session first.
      await syncSession().catch(() => undefined);
      source = new EventSource('/api/events');
      source.addEventListener('ready', (msg) => {
        const { tables } = JSON.parse((msg as MessageEvent<string>).data) as { tables: string[] };
        if (opened) dispatch('db://changed', { tables });
        opened = true;
        resolve();
      });
      source.onmessage = (msg) => {
        const { event, payload } = JSON.parse(msg.data) as { event: string; payload: unknown };
        dispatch(event, payload);
      };
      // The browser retries dropped streams itself, but not an error response.
      source.onerror = function (this: EventSource) {
        if (this.readyState === EventSource.CLOSED) setTimeout(() => void open(), RETRY_MS);
      };
    };
    void open();
  });
  return ready;
}

/** Talks to cvd-server: `POST /api/rpc/:command` and server-sent events on `/api/events`. */
export const transport: Transport = {
  async call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    const token = await accessToken();
    const res = await fetch(`/api/rpc/${cmd}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}) },
      body: JSON.stringify(args ?? {}),
    });
    const body = await res.text();
    const value = body ? JSON.parse(body) : null;
    // Error bodies are the same `{ kind, message }` Tauri rejects with.
    if (!res.ok) throw value ?? new Error(`${cmd} failed: ${res.status}`);
    return value as T;
  },
  async listen<T>(event: string, cb: (event: { payload: T }) => void) {
    await connect();
    const set = handlers.get(event) ?? new Set<Handler>();
    handlers.set(event, set);
    set.add(cb as Handler);
    return () => { set.delete(cb as Handler); };
  },
};
export const platform: Platform = {
  capabilities: { autoApply: false, updater: false, revealInFolder: false, localServer: false },
  auth,
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
