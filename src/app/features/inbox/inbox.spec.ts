import { TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';

import { ZardSidebarService } from '@/shared/components/sidebar/sidebar.service';
import { flush, mockTauri, type TauriMock } from '../../../testing/tauri-mock';
import type { ProcessedEmail, WatcherStatus } from '../../core/ipc/types';
import { InboxStore } from '../../core/stores/inbox.store';
import { ScanStore } from '../../core/stores/scan.store';
import { InboxPageComponent } from './inbox-page.component';

const STATUSES = [
  { id: 1, name: 'Applied', color: 'yellow', position: 1 },
  { id: 2, name: 'Interviewing', color: 'purple', position: 2 },
  { id: 3, name: 'Offer', color: 'green', position: 3 },
];

const job = (id: number, status_id: number | null) => ({
  id,
  title: 'Engineer',
  company: 'Acme',
  raw_text: '',
  created_at: '2026-10-01T00:00:00Z',
  url: '',
  status_id,
  company_id: null,
});

const row = (message_id: string, overrides: Partial<ProcessedEmail> = {}): ProcessedEmail => ({
  message_id,
  run_id: 1,
  processed_at: '2026-10-20 10:00:00',
  received_at: 'Tue, 20 Oct 2026 10:00:00 +0000',
  subject: 'Subject',
  sender: 'hr@acme.com',
  matched_job_id: 12,
  previous_status_id: 1,
  suggested_status_id: 2,
  confidence: 0.9,
  summary: `Summary of ${message_id}.`,
  outcome: 'applied',
  job_related: 0.9,
  body: `Body of ${message_id}`,
  read_at: null,
  undone_at: null,
  gmail_url: 'https://mail.google.com/',
  ...overrides,
});

const WATCHING: WatcherStatus = {
  state: 'watching',
  error: null,
  error_kind: null,
  last_checked_at: new Date().toISOString(),
};

async function render(
  handlers: Record<string, unknown> = {},
  url = '/inbox',
): Promise<{ t: TauriMock; el: HTMLElement; detect: () => Promise<void> }> {
  const t = mockTauri({
    list_jobs: [job(12, 2)],
    list_job_statuses: STATUSES,
    list_inbox: [row('<review>', { outcome: 'pending_review' }), row('<applied>')],
    unread_count: 2,
    get_watcher_status: WATCHING,
    get_scan_status: { running: false },
    list_pending_reviews: [],
    get_mail_account: { host: 'imap.gmail.com', port: 993, address: 'me@gmail.com', has_password: true, folder: '[Gmail]/All Mail' },
    mark_read: null,
    mark_all_read: null,
    apply_suggestion: row('<review>'),
    dismiss_suggestion: null,
    undo_applied: row('<applied>', { undone_at: '2026-10-20 12:00:00' }),
    retry_watcher: null,
    start_scan: 3,
    ...handlers,
  });
  TestBed.configureTestingModule({
    providers: [provideRouter([{ path: 'inbox', component: InboxPageComponent }]), ZardSidebarService],
  });
  await TestBed.inject(Router).navigateByUrl(url);
  const f = TestBed.createComponent(InboxPageComponent);
  const detect = async () => {
    f.detectChanges();
    await flush();
    f.detectChanges();
  };
  await detect();
  return { t, el: f.nativeElement as HTMLElement, detect };
}

const rows = (el: HTMLElement) => Array.from(el.querySelectorAll<HTMLElement>('[data-testid="inbox-row"]'));
const button = (scope: HTMLElement, label: string) =>
  Array.from(scope.querySelectorAll<HTMLButtonElement>('button')).find((b) => b.textContent?.trim() === label);
const isDisabled = (b: HTMLElement) =>
  (b as HTMLButtonElement).disabled || b.getAttribute('aria-disabled') === 'true' || b.hasAttribute('data-disabled');

describe('InboxPageComponent', () => {
  it('renders each row variant', async () => {
    const { el } = await render({
      list_inbox: [
        row('<review>', { outcome: 'pending_review', confidence: 0.7 }),
        row('<applied>'),
        row('<nochange>', { outcome: 'no_change', suggested_status_id: null }),
        row('<undone>', { undone_at: '2026-10-20 12:00:00' }),
        row('<dismissed>', { outcome: 'dismissed' }),
      ],
    });

    const [review, applied, noChange, undone, dismissed] = rows(el);
    expect(review.textContent).toContain('Interviewing');
    expect(review.textContent).toContain('70%');
    expect(button(review, 'Apply')).toBeTruthy();
    expect(button(review, 'Dismiss')).toBeTruthy();
    expect(applied.textContent).toContain('Applied');
    expect(button(applied, 'Undo')).toBeTruthy();
    expect(noChange.textContent).toContain('No change');
    expect(undone.textContent).toContain('Undone');
    expect(button(undone, 'Undo')).toBeFalsy();
    expect(dismissed.textContent).toContain('Dismissed');
  });

  it('keeps the backend order, reviews first', async () => {
    const { el } = await render();

    expect(rows(el).map((r) => r.dataset['messageId'])).toEqual(['<review>', '<applied>']);
  });

  it('shows an unread dot only on unread rows', async () => {
    const { el } = await render({ list_inbox: [row('<new>'), row('<old>', { read_at: '2026-10-20 11:00:00' })] });

    const [unread, read] = rows(el);
    expect(unread.querySelector('[data-testid="unread-dot"]')).toBeTruthy();
    expect(read.querySelector('[data-testid="unread-dot"]')).toBeNull();
  });

  it('Apply calls apply_suggestion', async () => {
    const { t, el, detect } = await render();

    button(rows(el)[0], 'Apply')!.click();
    await detect();

    expect(t.callsTo('apply_suggestion')).toEqual([{ messageId: '<review>' }]);
  });

  it('Undo calls undo_applied', async () => {
    const { t, el, detect } = await render();

    button(rows(el)[1], 'Undo')!.click();
    await detect();

    expect(t.callsTo('undo_applied')).toEqual([{ messageId: '<applied>' }]);
  });

  it('Undo disabled after manual status change shows why', async () => {
    const { el } = await render({ list_jobs: [job(12, 3)] });

    const applied = rows(el)[1];
    const undo = button(applied, 'Undo')!;
    expect(isDisabled(undo)).toBe(true);
    expect(applied.querySelector('[data-testid="undo-blocked"]')?.getAttribute('title')).toBe(
      'The status was changed after this update.',
    );
  });

  it('expanding a row shows the body and marks it read', async () => {
    const { t, el, detect } = await render();
    const applied = rows(el)[1];
    expect(applied.textContent).not.toContain('Body of <applied>');

    (applied.querySelector('[data-testid="row-toggle"]') as HTMLElement).click();
    await detect();

    expect(rows(el)[1].textContent).toContain('Body of <applied>');
    expect(t.callsTo('mark_read')).toEqual([{ messageId: '<applied>' }]);
  });

  it('tabs update the filter query param', async () => {
    const { t, el, detect } = await render();

    button(el, 'Needs review')!.click();
    await detect();

    expect(TestBed.inject(Router).url).toBe('/inbox?filter=review');
    expect(t.callsTo('list_inbox').at(-1)).toEqual({ filter: 'review' });
  });

  it('reads the filter from the URL', async () => {
    const { t } = await render({}, '/inbox?filter=unread');

    expect(t.callsTo('list_inbox').at(-1)).toEqual({ filter: 'unread' });
    expect(TestBed.inject(InboxStore).filter()).toBe('unread');
  });

  it('paused watcher shows error, settings link and Retry', async () => {
    const { t, el, detect } = await render({
      get_watcher_status: { state: 'paused', error: 'Gmail authentication failed', error_kind: 'MailAuth', last_checked_at: null },
    });

    const status = el.querySelector('[data-testid="watcher-status"]') as HTMLElement;
    expect(status.textContent).toContain('Paused: Gmail authentication failed');
    expect(status.querySelector('a')?.getAttribute('href')).toBe('/settings');
    button(status, 'Retry')!.click();
    await detect();
    expect(t.callsTo('retry_watcher')).toHaveLength(1);
  });

  it('a paused provider error links to AI models', async () => {
    const { el } = await render({
      get_watcher_status: { state: 'paused', error: 'Out of credits', error_kind: 'ProviderCredits', last_checked_at: null },
    });

    const status = el.querySelector('[data-testid="watcher-status"]') as HTMLElement;
    expect(status.querySelector('a')?.getAttribute('href')).toBe('/ai');
  });

  it('a watching status says when it last checked', async () => {
    const { el } = await render();

    expect(el.querySelector('[data-testid="watcher-status"]')!.textContent).toContain('Watching Gmail · checked just now');
  });

  it('no account says to connect Gmail and disables Check now', async () => {
    const { el } = await render({
      get_mail_account: null,
      get_watcher_status: { state: 'stopped', error: null, error_kind: null, last_checked_at: null },
    });

    expect(el.querySelector('[data-testid="watcher-status"]')!.textContent).toContain('Connect Gmail in Settings › Integrations.');
    expect(isDisabled(button(el, 'Check now')!)).toBe(true);
  });

  it('Check now disabled while running', async () => {
    const { el, detect } = await render();
    TestBed.inject(ScanStore).running.set(true);
    await detect();

    expect(isDisabled(button(el, 'Check now')!)).toBe(true);
  });

  it('Check now starts a scan', async () => {
    const { t, el, detect } = await render();

    button(el, 'Check now')!.click();
    await detect();

    expect(t.callsTo('start_scan')).toHaveLength(1);
  });

  it('Mark all read calls mark_all_read', async () => {
    const { t, el, detect } = await render();

    button(el, 'Mark all read')!.click();
    await detect();

    expect(t.callsTo('mark_all_read')).toHaveLength(1);
  });

  it('shows an empty state', async () => {
    const { el } = await render({ list_inbox: [] });

    expect(el.textContent).toContain('No job updates yet');
  });
});
