import { afterEach, describe, expect, it, vi } from 'vitest';
import { platform, transport } from './runtime';

afterEach(() => vi.restoreAllMocks());

describe('web platform', () => {
  it('disables desktop capabilities and updater checks', async () => {
    expect(Object.values(platform.capabilities)).toEqual([false, false, false, false]);
    expect(await platform.checkForUpdates()).toBeNull();
    await expect(transport.call('get_profile')).rejects.toThrow('Web transport is not connected');
    const unlisten = await transport.listen('db://changed', () => {});
    expect(() => unlisten()).not.toThrow();
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
