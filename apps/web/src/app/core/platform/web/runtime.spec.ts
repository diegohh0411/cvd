import { afterEach, describe, expect, it, vi } from 'vitest';
import { platform, transport } from './runtime';

afterEach(() => vi.restoreAllMocks());

describe('web platform', () => {
  it('disables desktop capabilities and updater checks', async () => {
    expect(Object.values(platform.capabilities)).toEqual([false, false, false, false]);
    expect(await platform.checkForUpdates()).toBeNull();
  });

  it('posts camelCase args to the RPC route and returns the JSON result', async () => {
    const fetch = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('[1,2]'));
    expect(await transport.call('list_runs', { jdId: 3 })).toEqual([1, 2]);
    expect(fetch).toHaveBeenCalledWith('/api/rpc/list_runs', expect.objectContaining({ method: 'POST', body: '{"jdId":3}' }));
    fetch.mockResolvedValue(new Response(''));
    expect(await transport.call('mark_all_read')).toBeNull();
  });

  it('rejects with the AppError body the server returns', async () => {
    const error = { kind: 'NotFound', message: 'Job not found.' };
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify(error), { status: 400 }));
    await expect(transport.call('get_job', { id: 9 })).rejects.toEqual(error);
  });

  it('waits for the server to subscribe, demuxes by event, and refreshes after a reconnect', async () => {
    type Listener = (m: { data: string }) => void;
    const sources: { onmessage: Listener | null; ready: Listener[] }[] = [];
    vi.stubGlobal('EventSource', class {
      onmessage: Listener | null = null;
      ready: Listener[] = [];
      constructor() { sources.push(this); }
      addEventListener(type: string, cb: Listener) { if (type === 'ready') this.ready.push(cb); }
    });
    try {
      const changed = vi.fn();
      const progress = vi.fn();
      let subscribed = false;
      const pending = transport.listen('db://changed', changed).then((u) => { subscribed = true; return u; });
      await Promise.resolve();
      expect(subscribed).toBe(false);
      const ready = (tables: string[]) => sources[0].ready.forEach((cb) => cb({ data: JSON.stringify({ tables }) }));
      ready(['entry', 'match_run']);
      const unlisten = await pending;
      await transport.listen('match://progress', progress);
      expect(sources).toHaveLength(1);
      // The first `ready` is the initial connection, not a reconnect.
      expect(changed).not.toHaveBeenCalled();

      sources[0].onmessage!({ data: JSON.stringify({ event: 'db://changed', payload: { tables: ['entry'] } }) });
      expect(changed).toHaveBeenCalledWith({ payload: { tables: ['entry'] } });
      expect(progress).not.toHaveBeenCalled();

      ready(['entry', 'match_run']);
      expect(changed).toHaveBeenLastCalledWith({ payload: { tables: ['entry', 'match_run'] } });

      unlisten();
      sources[0].onmessage!({ data: JSON.stringify({ event: 'db://changed', payload: { tables: [] } }) });
      expect(changed).toHaveBeenCalledTimes(2);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it('exports by downloading the PDF bytes the backend returns', async () => {
    const call = vi.spyOn(transport, 'call').mockResolvedValue({ id: 1, pdf: [37, 80, 68, 70] });
    const save = vi.spyOn(platform, 'saveFile').mockResolvedValue('CV.pdf');
    const selection = { entries: [], skills: [] };
    expect(await platform.exportCv(3, selection, 'CV.pdf')).toBe('CV.pdf');
    expect(call).toHaveBeenCalledWith('export_cv', { runId: 3, selection });
    expect(save).toHaveBeenCalledWith(new Uint8Array([37, 80, 68, 70]), 'CV.pdf');
  });

  it('downloads bytes with the suggested filename and releases the object URL', async () => {
    vi.useFakeTimers();
    const create = vi.spyOn(URL, 'createObjectURL').mockReturnValue('blob:test');
    const revoke = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
      expect(this.download).toBe('CV.pdf');
      expect(this.href).toBe('blob:test');
      expect(this.isConnected).toBe(true);
    });
    try {
      expect(await platform.saveFile(new Uint8Array([37, 80, 68, 70]), 'CV.pdf')).toBe('CV.pdf');
      expect(click).toHaveBeenCalledOnce();
      expect((create.mock.calls[0][0] as Blob).size).toBe(4);
      expect(document.querySelector('a[download]')).toBeNull();
      vi.runAllTimers();
      expect(revoke).toHaveBeenCalledWith('blob:test');
    } finally {
      vi.useRealTimers();
    }
  });
});
