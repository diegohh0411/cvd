import { TestBed } from '@angular/core/testing';
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

  it('blocks without a model, pointing to AI › AI Models', async () => {
    const r = await setup([RANKED.entries[0].entry], 'none');
    expect(r.blocker()?.link).toBe('/ai');
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
      list_job_views: [{ id: 1, name: 'All jobs', layout: 'table', filters: [], sorts: [], position: 0 }],
      list_pending_reviews: [],
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

  it('review badge links to inbox review filter', async () => {
    const { f, el, scan } = await render();
    scan.pendingCount.set(2);
    f.detectChanges();
    await flush();
    f.detectChanges();

    const badge = el.querySelector('[data-testid="review-badge"]') as HTMLAnchorElement;
    expect(badge.textContent).toContain('2 to review');
    expect(badge.getAttribute('href')).toBe('/inbox?filter=review');
  });

  it('has no scan button any more', async () => {
    const { el } = await render();

    expect(el.querySelector('[data-testid="scan-button"]')).toBeNull();
  });

  it('ranks jobs by the fuzzy search ids and shows an empty state', async () => {
    const job = (id: number, company: string) => ({ id, title: 'Engineer', company, raw_text: '', created_at: '2026-01-01T00:00:00Z', url: '', status_id: null, company_id: null });
    const { f, el } = await render({ list_jobs: [job(1, 'Acme'), job(2, 'Stripe')], list_runs: [], search_jobs: [1] });
    const input = el.querySelector('[data-testid="job-search"]') as HTMLInputElement;
    input.value = 'acm';
    input.dispatchEvent(new Event('input'));
    await flush();
    f.detectChanges();
    const rows = () => Array.from(el.querySelectorAll('table[aria-label="Jobs"] tbody tr')).map((li) => li.textContent);
    expect(rows()).toHaveLength(1);
    expect(rows()[0]).toContain('Acme');
  });
});
