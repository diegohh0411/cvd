// Test double for the Tauri bridge: commands are answered by `handlers`,
// and `emit()` delivers events to `listen()` subscribers.
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks';

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
  clearMocks();
  mockIPC(
    (cmd, payload) => {
      const args = (payload ?? {}) as Record<string, unknown>;
      if (cmd.startsWith('plugin:event|')) return undefined;
      calls.push({ cmd, args });
      const h = table[cmd];
      if (!h) throw new Error(`unmocked command ${cmd}`);
      return h(args);
    },
    { shouldMockEvents: true },
  );
  return {
    calls,
    set,
    callsTo: (cmd) => calls.filter((c) => c.cmd === cmd).map((c) => c.args),
    async emit(event, payload) {
      const { emit } = await import('@tauri-apps/api/event');
      await emit(event, payload);
      await flush();
    },
  };
}

/** Lets pending promises and signal effects settle. */
export async function flush(times = 5): Promise<void> {
  for (let i = 0; i < times; i++) await new Promise((r) => setTimeout(r, 0));
}
