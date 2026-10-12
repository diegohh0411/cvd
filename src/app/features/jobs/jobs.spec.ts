import { TestBed } from '@angular/core/testing';
import { By } from '@angular/platform-browser';
import { Router } from '@angular/router';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri } from '../../../testing/tauri-mock';
import { EngineStore } from '../../core/stores/engine.store';
import { LibraryStore } from '../../core/stores/library.store';
import { ScanStore } from '../../core/stores/scan.store';
import { RANKED } from '../workspace/fixtures';
import { NewJobComponent, wordCount } from './new-job.component';
import { JobsPageComponent } from './jobs-page.component';
import { MatchReadiness } from './match-readiness';
import { ScanResultsDialogComponent } from './scan-results-dialog.component';

describe('match readiness', () => {
  function setup(entries: unknown[], state: string) {
    mockTauri({ list_entries: entries, list_skills: [], engine_status: { model_id: state === 'none' ? null : 'x', state, error: null } });
    TestBed.configureTestingModule({ providers: pageProviders() });
    return Promise.all([TestBed.inject(LibraryStore).load(), TestBed.inject(EngineStore).load()]).then(() => TestBed.inject(MatchReadiness));
  }

  it('blocks on an empty library first, pointing to the Library', async () => {
    const r = await setup([], 'none');
    expect(r.blocker()?.link).toBe('/library/experience');
  });

  it('blocks without a model, pointing to AI › Models', async () => {
    const r = await setup([RANKED.entries[0].entry], 'none');
    expect(r.blocker()?.link).toBe('/ai/models');
    expect(r.blocker()?.queryParams).toBeUndefined();
  });

  it('is ready with entries and an active model', async () => {
    const r = await setup([RANKED.entries[0].entry], 'idle');
    expect(r.blocker()).toBeNull();
  });
});

describe('NewJobComponent', () => {
  it('counts words', () => {
    expect(wordCount('  a b\nc ')).toBe(3);
    expect(wordCount('')).toBe(0);
  });

  it('disables Match on an empty library and links to the Library', async () => {
    mockTauri({ list_entries: [], list_skills: [], engine_status: { model_id: 'x', state: 'idle', error: null } });
    TestBed.configureTestingModule({ providers: pageProviders() });
    await TestBed.inject(LibraryStore).load();
    await TestBed.inject(EngineStore).load();
    const f = TestBed.createComponent(NewJobComponent);
    f.detectChanges();
    await flush();
    f.detectChanges();
    const el = f.nativeElement as HTMLElement;
    expect(el.textContent).toContain('Your library is empty');
    const match = [...el.querySelectorAll('button')].find((b) => b.textContent?.includes('Match'))!;
    expect(match.getAttribute('aria-disabled') === 'true' || match.disabled || match.hasAttribute('data-disabled')).toBe(true);
  });

  it('creates the job and opens the workspace with auto-start', async () => {
    const t = mockTauri({
      list_entries: [RANKED.entries[0].entry],
      list_skills: [],
      engine_status: { model_id: 'x', state: 'idle', error: null },
      create_job: 42,
      list_jobs: [],
    });
    TestBed.configureTestingModule({ providers: pageProviders() });
    await TestBed.inject(LibraryStore).load();
    await TestBed.inject(EngineStore).load();
    const nav = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);
    const f = TestBed.createComponent(NewJobComponent);
    f.detectChanges();
    await flush(); // ngModel controls register asynchronously inside a <form>
    const text = (f.nativeElement as HTMLElement).querySelector('#jd-text') as HTMLTextAreaElement;
    text.value = 'We need Rust';
    text.dispatchEvent(new Event('input'));
    await flush();
    await (f.componentInstance as unknown as { match(): Promise<void> }).match();
    expect(t.callsTo('create_job')).toEqual([{ title: '', company: '', rawText: 'We need Rust', url: '', statusId: null }]);
    expect(nav).toHaveBeenCalledWith(['/jobs', 42], { queryParams: { start: 1 } });
  });

  it('reads the description from the posting URL when none is pasted, then matches', async () => {
    const t = mockTauri({
      list_entries: [RANKED.entries[0].entry],
      list_skills: [],
      engine_status: { model_id: 'x', state: 'idle', error: null },
      fetch_job_description: 'We need Rust for payments.',
      create_job: 42,
      list_jobs: [],
    });
    TestBed.configureTestingModule({ providers: pageProviders() });
    await TestBed.inject(LibraryStore).load();
    await TestBed.inject(EngineStore).load();
    const nav = vi.spyOn(TestBed.inject(Router), 'navigate').mockResolvedValue(true);
    const f = TestBed.createComponent(NewJobComponent);
    f.detectChanges();
    await flush();
    const el = f.nativeElement as HTMLElement;
    const url = el.querySelector('#jd-url') as HTMLInputElement;
    url.value = 'https://acme.com/careers/1';
    url.dispatchEvent(new Event('input'));
    await flush();
    f.detectChanges();
    expect(el.querySelector('button[type=submit]')!.textContent).toContain('Fetch description');

    const cmp = f.componentInstance as unknown as { submit(): Promise<void> };
    await cmp.submit();
    f.detectChanges();
    await flush();
    f.detectChanges();

    expect(t.callsTo('fetch_job_description')).toEqual([{ url: 'https://acme.com/careers/1' }]);
    expect(t.callsTo('create_job')).toEqual([]);
    expect((el.querySelector('#jd-text') as HTMLTextAreaElement).value).toBe('We need Rust for payments.');
    expect(el.querySelector('button[type=submit]')!.textContent).toContain('Match');

    await cmp.submit();
    expect(t.callsTo('create_job')).toEqual([
      { title: '', company: '', rawText: 'We need Rust for payments.', url: 'https://acme.com/careers/1', statusId: null },
    ]);
    expect(nav).toHaveBeenCalledWith(['/jobs', 42], { queryParams: { start: 1 } });
  });
});

describe('JobsPageComponent scan controls', () => {
  const ACCOUNT = { host: 'imap.gmail.com', port: 993, address: 'diego@gmail.com', has_password: true };

  async function render(handlers: Record<string, unknown> = {}) {
    mockTauri({
      list_jobs: [],
      list_job_statuses: [],
      list_pending_reviews: [],
      list_scan_results: [],
      get_scan_status: { running: false },
      get_mail_account: ACCOUNT,
      ...handlers,
    });
    TestBed.configureTestingModule({ providers: pageProviders() });
    const f = TestBed.createComponent(JobsPageComponent);
    f.detectChanges();
    await flush();
    f.detectChanges();
    return { f, el: f.nativeElement as HTMLElement, scan: TestBed.inject(ScanStore) };
  }

  const isDisabled = (b: HTMLElement) =>
    (b as HTMLButtonElement).disabled || b.getAttribute('aria-disabled') === 'true' || b.hasAttribute('data-disabled');

  it('disables the button while running and shows the count', async () => {
    const { f, el, scan } = await render();
    scan.running.set(true);
    scan.progress.set({ checked: 4, total: 10 });
    f.detectChanges();
    await flush();
    f.detectChanges();

    const button = el.querySelector('[data-testid="scan-button"]') as HTMLElement;
    expect(button.textContent).toContain('Reading emails 4 / 10');
    expect(isDisabled(button)).toBe(true);
  });

  it('disables the button with a connect hint when no account exists', async () => {
    const { f, el } = await render({ get_mail_account: null });
    f.detectChanges();
    await flush();
    f.detectChanges();

    const button = el.querySelector('[data-testid="scan-button"]') as HTMLElement;
    expect(isDisabled(button)).toBe(true);
    expect(el.textContent).toContain('Connect Gmail in Settings › Integrations');
  });

  it('shows the pending count on the badge, enabled even while a scan runs', async () => {
    const { f, el, scan } = await render();
    scan.pendingCount.set(2);
    scan.running.set(true);
    f.detectChanges();
    await flush();
    f.detectChanges();

    const badge = el.querySelector('[data-testid="review-badge"]') as HTMLElement;
    expect(badge.textContent).toContain('2 to review');
    expect(isDisabled(badge)).toBe(false);
  });

  it('opens the results dialog once when a scan finishes', async () => {
    const { f, scan } = await render();
    scan.finishedRunId.set(5);
    f.detectChanges();
    await flush();
    f.detectChanges();

    const page = f.componentInstance as unknown as { dialogOpen: () => boolean; dialogRunId: () => number | null };
    expect(page.dialogOpen()).toBe(true);
    expect(page.dialogRunId()).toBe(5);
    expect(scan.finishedRunId()).toBeNull();
    expect(f.debugElement.query(By.directive(ScanResultsDialogComponent))).toBeTruthy();
  });
});
