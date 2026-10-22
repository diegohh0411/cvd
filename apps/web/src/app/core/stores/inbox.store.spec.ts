import { TestBed } from '@angular/core/testing';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri, type TauriMock } from '../../../testing/tauri-mock';
import type { ProcessedEmail, WatcherStatus } from '../ipc/types';
import { InboxStore } from './inbox.store';

const row = (message_id: string, overrides: Partial<ProcessedEmail> = {}): ProcessedEmail => ({
  message_id,
  run_id: 1,
  processed_at: '2026-10-20 10:00:00',
  received_at: 'Tue, 20 Oct 2026 10:00:00 +0000',
  subject: 'Subject',
  sender: 'hr@acme.com',
  matched_job_id: 1,
  previous_status_id: 1,
  suggested_status_id: 2,
  confidence: 0.9,
  summary: 'Summary.',
  outcome: 'applied',
  job_related: 0.9,
  body: 'Body',
  read_at: null,
  undone_at: null,
  gmail_url: 'https://mail.google.com/',
  ...overrides,
});

const WATCHING: WatcherStatus = { state: 'watching', error: null, error_kind: null, last_checked_at: '2026-10-20T10:00:00Z' };

async function setup(extra: Record<string, unknown> = {}): Promise<{ t: TauriMock; store: InboxStore }> {
  const t = mockTauri({
    list_inbox: [row('<a>'), row('<b>', { read_at: '2026-10-20 11:00:00' })],
    unread_count: 1,
    get_watcher_status: WATCHING,
    mark_read: null,
    mark_all_read: null,
    undo_applied: row('<a>', { undone_at: '2026-10-20 12:00:00' }),
    apply_suggestion: row('<a>'),
    dismiss_suggestion: null,
    retry_watcher: null,
    ...extra,
  });
  TestBed.configureTestingModule({ providers: pageProviders() });
  const store = TestBed.inject(InboxStore);
  await store.ensureReady();
  return { t, store };
}

describe('InboxStore', () => {
  it('loads items, unread count and watcher status on ensureReady', async () => {
    const { t, store } = await setup();

    expect(store.items().map((r) => r.message_id)).toEqual(['<a>', '<b>']);
    expect(store.unread()).toBe(1);
    expect(store.watcher()).toEqual(WATCHING);
    expect(t.callsTo('list_inbox')).toEqual([{ filter: 'all' }]);
  });

  it('refreshes on inbox://changed', async () => {
    const { t, store } = await setup();
    t.set('list_inbox', [row('<c>')]);
    t.set('unread_count', 3);

    await t.emit('inbox://changed', { message_id: '<c>' });

    expect(store.items().map((r) => r.message_id)).toEqual(['<c>']);
    expect(store.unread()).toBe(3);
  });

  it('refreshes when a scan finishes', async () => {
    const { t, store } = await setup();
    t.set('unread_count', 5);

    await t.emit('scan://finished', { run_id: 2, source: 'watch', status: 'done' });

    expect(store.unread()).toBe(5);
  });

  it('setFilter re-queries with the filter', async () => {
    const { t, store } = await setup();

    await store.setFilter('review');

    expect(store.filter()).toBe('review');
    expect(t.callsTo('list_inbox').at(-1)).toEqual({ filter: 'review' });
  });

  it('undo calls undo_applied and refreshes', async () => {
    const { t, store } = await setup();

    await store.undo('<a>');

    expect(t.callsTo('undo_applied')).toEqual([{ messageId: '<a>' }]);
    expect(t.callsTo('list_inbox')).toHaveLength(2);
  });

  it('markRead skips rows already read', async () => {
    const { t, store } = await setup();

    await store.markRead('<b>');
    await store.markRead('<a>');

    expect(t.callsTo('mark_read')).toEqual([{ messageId: '<a>' }]);
  });

  it('tracks watcher status events', async () => {
    const { t, store } = await setup();
    const paused: WatcherStatus = { state: 'paused', error: 'Gmail authentication failed', error_kind: 'MailAuth', last_checked_at: null };

    await t.emit('mail://watcher', paused);
    await flush();

    expect(store.watcher()).toEqual(paused);
  });

  it('retry calls retry_watcher', async () => {
    const { t, store } = await setup();

    await store.retry();

    expect(t.callsTo('retry_watcher')).toHaveLength(1);
  });
});
