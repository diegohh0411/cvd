import { TestBed } from '@angular/core/testing';

import { pageProviders } from '../../../testing/providers';
import { mockTauri, type TauriMock } from '../../../testing/tauri-mock';
import type { Company, JobDescription, MatchRun, ProviderModels } from '../../core/ipc/types';
import { RANKED } from '../workspace/fixtures';
import { WorkspaceStore } from '../workspace/workspace.store';
import { contextBadge, needsRefreshConfirm } from './company-panel.component';
import { researchLine, stepLine } from './research-progress.component';

const JOB: JobDescription = { id: 7, title: 'Dev', company: 'Acme', raw_text: 'Rust', created_at: '', url: '', status_id: null, company_id: 5 };
const run = (status: MatchRun['status'], researched: string | null = null): MatchRun => ({
  id: 3,
  jd_id: 7,
  model_id: 'clef-flash',
  status,
  created_at: '',
  company_researched_at: researched,
});
const company = (over: Partial<Company> = {}): Company => ({
  id: 5,
  name: 'Acme',
  normalized_name: 'acme',
  domain: 'acme.com',
  profile: { summary: '', industry: 'Fintech', size: '', stage: '', headquarters: '', remote_policy: '', products: '', tech_stack: [], sources: [] },
  status: 'ok',
  researched_at: '2026-10-08 10:00:00',
  model_id: 'lfm',
  fail_count: 0,
  last_error: '',
  retry_after: null,
  edited_at: null,
  ...over,
});

describe('research progress copy', () => {
  it('formats the status line', () => {
    expect(researchLine('Acme', { n: 7, max: 25 }, 102)).toBe('Researching Acme… step 7/25 · 1:42');
    expect(researchLine('Acme', null, 5)).toBe('Researching Acme… · 0:05');
  });

  it('describes agent steps', () => {
    expect(stepLine({ kind: 'searching', query: 'acme funding' })).toBe('Searching “acme funding”');
    expect(stepLine({ kind: 'reading', domain: 'acme.com' })).toBe('Reading acme.com');
  });
});

describe('company context badge', () => {
  it('shows when a finished run had no company context', () => {
    expect(contextBadge(run('done'), company())).toEqual({ show: true, canRescore: true, label: 'Scored without company context' });
    expect(contextBadge(run('done'), company({ status: 'pending', researched_at: null })).canRescore).toBe(false);
    expect(contextBadge(run('done'), null).canRescore).toBe(false);
    expect(contextBadge(run('running'), company()).show).toBe(false);
  });

  it('flags a profile updated since scoring', () => {
    const b = contextBadge(run('done', '2026-10-01 10:00:00'), company());
    expect(b).toEqual({ show: true, canRescore: true, label: 'Company profile updated since scoring' });
    expect(contextBadge(run('done', '2026-10-08 10:00:00'), company()).show).toBe(false);
  });

  it('asks before a refresh overwrites manual edits', () => {
    expect(needsRefreshConfirm(company({ edited_at: '2026-10-08 11:00:00' }))).toBe(true);
    expect(needsRefreshConfirm(company())).toBe(false);
  });
});

describe('WorkspaceStore research', () => {
  function setup(handlers: Record<string, unknown> = {}): { t: TauriMock; ws: WorkspaceStore } {
    const t = mockTauri({
      get_job: JOB,
      list_runs: [run('running')],
      get_ui_setting: null,
      get_ranked: RANKED,
      get_scores: [],
      fit_cv: { selection: { entries: [], skills: [] }, pages: 1, fill: 0.5, overflow: false, pdf_base64: '' },
      list_jobs: [],
      get_job_company: company({ status: 'pending', researched_at: null }),
      skip_research: null,
      ...handlers,
    });
    TestBed.configureTestingModule({ providers: [...pageProviders(), WorkspaceStore] });
    return { t, ws: TestBed.inject(WorkspaceStore) };
  }

  it('tracks research steps for the run’s company', async () => {
    const { ws, t } = setup();
    await ws.init(7, false);
    await t.emit('match://progress', { run_id: 3, event: { stage: 'researching', company_id: 5 } });
    expect(ws.research()?.companyId).toBe(5);
    await t.emit('company://research', { company_id: 5, event: { stage: 'step', step: { kind: 'round', n: 7, max: 25 } } });
    await t.emit('company://research', { company_id: 5, event: { stage: 'step', step: { kind: 'searching', query: 'acme' } } });
    await t.emit('company://research', { company_id: 9, event: { stage: 'step', step: { kind: 'round', n: 1, max: 25 } } });
    expect(ws.research()?.round).toEqual({ n: 7, max: 25 });
    expect(ws.research()?.log).toEqual(['Searching “acme”']);
    await t.emit('match://progress', { run_id: 3, event: { stage: 'reading_jd' } });
    expect(ws.research()).toBeNull();
  });

  it('skips research in either mode', async () => {
    const { ws, t } = setup();
    await ws.init(7, false);
    await t.emit('match://progress', { run_id: 3, event: { stage: 'researching', company_id: 5 } });
    await ws.skipResearch('cancel');
    await ws.skipResearch('background');
    expect(t.callsTo('skip_research')).toEqual([
      { runId: 3, mode: 'cancel' },
      { runId: 3, mode: 'background' },
    ]);
  });

  it('reloads the run when it finishes so the context badge is accurate', async () => {
    const { ws, t } = setup();
    await ws.init(7, false);
    t.set('list_runs', [run('done')]);
    await t.emit('match://progress', { run_id: 3, event: { stage: 'done' } });
    expect(ws.run()?.company_researched_at).toBeNull();
    expect(ws.company()?.id).toBe(5);
  });

  it('re-scores into a new run', async () => {
    const { ws, t } = setup({ list_runs: [run('done')], rescore: 4 });
    await ws.init(7, false);
    await ws.rescore();
    expect(t.callsTo('rescore')).toEqual([{ runId: 3 }]);
    expect(ws.run()?.id).toBe(4);
    expect(ws.phase()).toBe('matching');
  });
});
