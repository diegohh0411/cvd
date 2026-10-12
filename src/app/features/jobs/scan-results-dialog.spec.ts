import { TestBed } from '@angular/core/testing';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri, type TauriMock } from '../../../testing/tauri-mock';
import type { ProcessedEmail } from '../../core/ipc/types';
import { JobStatusesStore } from '../../core/stores/job-statuses.store';
import { JobsStore } from '../../core/stores/jobs.store';
import { ScanResultsDialogComponent } from './scan-results-dialog.component';

const STATUSES = [
  { id: 1, name: 'Applied', color: 'yellow', position: 1 },
  { id: 2, name: 'Interviewing', color: 'purple', position: 2 },
];

const JOBS = [
  { id: 12, title: 'Engineer', company: 'Acme', raw_text: '', created_at: '2026-10-01T00:00:00Z', url: '', status_id: 1, company_id: null },
  // No company and no URL: the label falls back to the title.
  { id: 13, title: 'Designer', company: '', raw_text: '', created_at: '2026-10-02T00:00:00Z', url: '', status_id: null, company_id: null },
];

const row = (overrides: Partial<ProcessedEmail>): ProcessedEmail => ({
  message_id: '<a>',
  run_id: 5,
  processed_at: '2026-10-12T09:00:00Z',
  received_at: '2026-10-10T09:00:00Z',
  subject: 'Subject',
  sender: 'recruiter@acme.com',
  matched_job_id: 12,
  previous_status_id: 1,
  suggested_status_id: 2,
  confidence: 0.9,
  summary: 'Summary.',
  outcome: 'applied',
  gmail_url: 'https://mail.google.com/mail/u/0/#search/rfc822msgid%3Aa',
  ...overrides,
});

async function render(handlers: Record<string, unknown>): Promise<{ t: TauriMock; text: () => string; host: HTMLElement }> {
  const t = mockTauri({
    list_jobs: JOBS,
    list_runs: [],
    list_job_statuses: STATUSES,
    get_scan_status: { running: false },
    list_scan_results: [],
    list_pending_reviews: [],
    ...handlers,
  });
  TestBed.configureTestingModule({ providers: pageProviders() });
  await TestBed.inject(JobsStore).load();
  await TestBed.inject(JobStatusesStore).ensureLoaded();

  const f = TestBed.createComponent(ScanResultsDialogComponent);
  f.componentRef.setInput('runId', 5);
  f.componentRef.setInput('open', true);
  f.detectChanges();
  await flush();
  f.detectChanges();

  // The dialog may render through an overlay, so read the whole document.
  return { t, text: () => document.body.textContent ?? '', host: document.body };
}

describe('ScanResultsDialogComponent', () => {
  it('renders the Applied, Needs review and No change sections with their rows', async () => {
    const { text, host } = await render({
      list_scan_results: [
        row({ message_id: '<a>', outcome: 'applied', summary: 'Interview invite.' }),
        row({ message_id: '<c>', outcome: 'no_change', suggested_status_id: 1, summary: 'Still waiting.' }),
      ],
      list_pending_reviews: [
        row({
          message_id: '<b>',
          outcome: 'pending_review',
          matched_job_id: 13,
          previous_status_id: null,
          suggested_status_id: 2,
          confidence: 0.6,
          summary: 'Maybe an interview.',
        }),
      ],
    });

    expect(text()).toContain('Applied');
    expect(text()).toContain('Needs review');
    expect(text()).toContain('No change');
    expect(text()).toContain('Interview invite.');
    expect(text()).toContain('Maybe an interview.');
    expect(text()).toContain('60% confident');
    expect(text()).toContain('Designer');
    expect(host.querySelectorAll('[aria-label="Open original email in Gmail"]').length).toBe(3);
  });

  it('Apply calls applySuggestion and removes the row', async () => {
    const { t, text } = await render({
      list_pending_reviews: [row({ message_id: '<b>', outcome: 'pending_review', summary: 'Maybe an interview.', confidence: 0.6 })],
      apply_suggestion: row({ message_id: '<b>', outcome: 'applied' }),
    });
    const apply = [...document.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Apply')!;

    apply.click();
    await flush();

    expect(t.callsTo('apply_suggestion')).toEqual([{ messageId: '<b>' }]);
    expect(text()).not.toContain('Maybe an interview.');
  });

  it('Dismiss calls dismissSuggestion and removes the row', async () => {
    const { t, text } = await render({
      list_pending_reviews: [row({ message_id: '<b>', outcome: 'pending_review', summary: 'Maybe an interview.', confidence: 0.6 })],
      dismiss_suggestion: null,
    });
    const dismiss = [...document.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Dismiss')!;

    dismiss.click();
    await flush();

    expect(t.callsTo('dismiss_suggestion')).toEqual([{ messageId: '<b>' }]);
    expect(text()).not.toContain('Maybe an interview.');
  });

  it('shows the empty state when the scan found nothing', async () => {
    const { text } = await render({});

    expect(text()).toContain('No job updates found in the last 30 days.');
  });
});
