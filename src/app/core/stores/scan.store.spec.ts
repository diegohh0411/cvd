import { TestBed } from '@angular/core/testing';

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

  it('a done scan clears running, records the run, and refreshes the pending count', async () => {
    const { t, store } = await setup({ list_pending_reviews: PENDING });
    await store.start();
    expect(store.running()).toBe(true);

    await t.emit('scan://finished', finished());

    expect(store.running()).toBe(false);
    expect(store.finishedRunId()).toBe(7);
    expect(store.finishedSummary()).toEqual({
      run_id: 7,
      checked: 12,
      job_related: 3,
      applied: 1,
      pending: 1,
      no_change: 1,
      failed: 0,
    });
    expect(store.lastError()).toBeNull();
    expect(store.pendingCount()).toBe(2);
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
});
