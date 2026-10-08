import { TestBed } from '@angular/core/testing';
import { Router } from '@angular/router';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri } from '../../../testing/tauri-mock';
import { EngineStore } from '../../core/stores/engine.store';
import { LibraryStore } from '../../core/stores/library.store';
import { RANKED } from '../workspace/fixtures';
import { NewJobComponent, wordCount } from './new-job.component';
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

  it('blocks without a model, pointing to Models › Download', async () => {
    const r = await setup([RANKED.entries[0].entry], 'none');
    expect(r.blocker()).toMatchObject({ link: '/models', queryParams: { tab: 'download' } });
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
    expect(t.callsTo('create_job')).toEqual([{ title: '', company: '', rawText: 'We need Rust' }]);
    expect(nav).toHaveBeenCalledWith(['/jobs', 42], { queryParams: { start: 1 } });
  });
});
