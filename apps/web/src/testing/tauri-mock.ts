// Test double for the Tauri bridge: commands are answered by `handlers`,
// and `emit()` delivers events to `listen()` subscribers.
import { transport } from '../app/core/platform/runtime';

export type Handler = (args: Record<string, unknown>) => unknown;

export interface TauriMock {
  calls: { cmd: string; args: Record<string, unknown> }[];
  set(cmd: string, h: Handler | unknown): void;
  emit(event: string, payload: unknown): Promise<void>;
  callsTo(cmd: string): Record<string, unknown>[];
}

export function mockTauri(handlers: Record<string, Handler | unknown> = {}): TauriMock {
  const table: Record<string, Handler> = {};
  const set = (cmd: string, h: Handler | unknown) => {
    table[cmd] = typeof h === 'function' ? (h as Handler) : () => h;
  };
  Object.entries(handlers).forEach(([k, v]) => set(k, v));
  const calls: TauriMock['calls'] = [];
  const listeners = new Map<string, Set<(event: { payload: unknown }) => void>>();
  transport.call = async <T>(cmd: string, payload?: Record<string, unknown>): Promise<T> => {
    const args = payload ?? {};
    calls.push({ cmd, args });
    const h = table[cmd];
    if (!h) throw new Error(`unmocked command ${cmd}`);
    return await h(args) as T;
  };
  transport.listen = async <T>(event: string, cb: (event: { payload: T }) => void) => {
    const callbacks = listeners.get(event) ?? new Set();
    listeners.set(event, callbacks);
    const callback = cb as (event: { payload: unknown }) => void;
    callbacks.add(callback);
    return () => { callbacks.delete(callback); };
  };
  return {
    calls,
    set,
    callsTo: (cmd) => calls.filter((c) => c.cmd === cmd).map((c) => c.args),
    async emit(event, payload) {
      for (const cb of listeners.get(event) ?? []) cb({ payload });
      await flush();
    },
  };
}

/** Lets pending promises and signal effects settle. */
export async function flush(times = 5): Promise<void> {
  for (let i = 0; i < times; i++) await new Promise((r) => setTimeout(r, 0));
}
