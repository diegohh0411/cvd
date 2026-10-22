import { TestBed } from '@angular/core/testing';

import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri, type TauriMock } from '../../../testing/tauri-mock';
import { ScanStore } from './scan.store';

const PENDING = [{ message_id: '<a>' }, { message_id: '<b>' }];

async function setup(extra: Record<string, unknown> = {}): Promise<{ t: TauriMock; store: ScanStore }> {
  const t = mockTauri({
    get_scan_status: { running: false },
    list_pending_reviews: [],
    start_scan: 7,
    ...extra,
  });
  TestBed.configureTestingModule({ providers: pageProviders() });
  const store = TestBed.inject(ScanStore);
  await store.ensureReady();
  return { t, store };
}

const finished = (overrides: Record<string, unknown> = {}) => ({
  run_id: 7,
  source: 'manual',
  status: 'done',
  error: null,
  error_kind: null,
  checked: 12,
  job_related: 3,
  applied: 1,
  pending: 1,
  no_change: 1,
  failed: 0,
  ...overrides,
});

describe('ScanStore', () => {
  it('a progress event updates progress', async () => {
    const { t, store } = await setup();

    await t.emit('scan://progress', { checked: 3, total: 10 });

    expect(store.progress()).toEqual({ checked: 3, total: 10 });
  });

  it('a done scan clears running and refreshes the pending count', async () => {
    const { t, store } = await setup({ list_pending_reviews: PENDING });
    await store.start();
    expect(store.running()).toBe(true);

    await t.emit('scan://finished', finished());

    expect(store.running()).toBe(false);
    expect(store.lastError()).toBeNull();
    expect(store.pendingCount()).toBe(2);
  });

  it('toasts once for a manual scan', async () => {
    const { t, store } = await setup();
    const toast = TestBed.inject(ZardSonnerService);
    const success = vi.spyOn(toast, 'success');
    await store.start();

    await t.emit('scan://finished', finished());
    await t.emit('scan://finished', finished({ source: 'watch' }));

    expect(success).toHaveBeenCalledTimes(1);
    expect(success).toHaveBeenCalledWith('2 updates');
  });

  it('a manual scan with nothing new says so', async () => {
    const { t, store } = await setup();
    const info = vi.spyOn(TestBed.inject(ZardSonnerService), 'info');
    await store.start();

    await t.emit('scan://finished', finished({ applied: 0, pending: 0 }));

    expect(info).toHaveBeenCalledWith('No new updates');
  });

  it('a failed manual scan toasts the error', async () => {
    const { t, store } = await setup();
    const error = vi.spyOn(TestBed.inject(ZardSonnerService), 'error');
    await store.start();

    await t.emit('scan://finished', finished({ status: 'failed', error: 'Gmail authentication failed' }));

    expect(error).toHaveBeenCalledWith('The check stopped', { description: 'Gmail authentication failed' });
  });

  it('does not toast for watch scans', async () => {
    const { t } = await setup();
    const toast = TestBed.inject(ZardSonnerService);
    const success = vi.spyOn(toast, 'success');
    const info = vi.spyOn(toast, 'info');

    await t.emit('scan://finished', finished({ source: 'watch' }));

    expect(success).not.toHaveBeenCalled();
    expect(info).not.toHaveBeenCalled();
  });

  it('a watch scan starting elsewhere shows as running from progress', async () => {
    const { t, store } = await setup();

    await t.emit('scan://progress', { checked: 0, total: 4 });

    expect(store.running()).toBe(true);
  });

  it('a failed scan sets lastError and its kind', async () => {
    const { t, store } = await setup();
    await store.start();

    await t.emit(
      'scan://finished',
      finished({ status: 'failed', error: 'Gmail authentication failed', error_kind: 'MailAuth' }),
    );

    expect(store.lastError()).toBe('Gmail authentication failed');
    expect(store.lastErrorKind()).toBe('MailAuth');
    expect(store.running()).toBe(false);
  });

  it('start() while already running is a no-op', async () => {
    const { t, store } = await setup();

    await store.start();
    await store.start();

    expect(t.callsTo('start_scan')).toHaveLength(1);
  });

  it('a re-check finishing does not end the running cycle', async () => {
    const { t, store } = await setup();
    await store.start();

    await t.emit('scan://finished', finished({ source: 'recheck' }));

    expect(store.running()).toBe(true);
  });

  it('start while the backend is already scanning is quiet', async () => {
    const { t, store } = await setup();
    t.set('start_scan', () => {
      throw { kind: 'Io', message: 'A scan is already running.' };
    });

    await expect(store.start()).resolves.toBeUndefined();
    expect(store.running()).toBe(true);
  });
});
