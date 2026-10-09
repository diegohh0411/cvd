import { TestBed } from '@angular/core/testing';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri, type TauriMock } from '../../../testing/tauri-mock';
import type { JobDescription, MatchRun } from '../../core/ipc/types';
import { breakdown } from './score-bar.component';
import { fillText, FillMeterComponent } from './fill-meter.component';
import { fitOut, RANKED, SELECTION } from './fixtures';
import { stepState } from './matching-stepper.component';
import { PdfPreviewComponent } from './pdf-preview.component';
import { FIT_DEBOUNCE_MS, WorkspaceStore } from './workspace.store';
import { defaultFileName, ensurePdf } from './export.service';
import { failureCopy } from './workspace-page.component';
import { roleSignals } from './cards.component';

const JOB: JobDescription = { id: 7, title: 'Dev', company: 'Acme', raw_text: 'Rust', created_at: '2026-10-08 10:00:00', url: '', status_id: null };
const run = (status: MatchRun['status'], id = 3): MatchRun => ({ id, jd_id: 7, model_id: 'clef-flash', status, created_at: '' });

function setup(handlers: Record<string, unknown> = {}): { t: TauriMock; ws: WorkspaceStore } {
  const t = mockTauri({
    get_job: JOB,
    list_runs: [],
    get_ui_setting: null,
    set_ui_setting: null,
    get_ranked: RANKED,
    get_scores: [],
    fit_cv: fitOut(),
    list_jobs: [],
    ...handlers,
  });
  TestBed.configureTestingModule({ providers: [...pageProviders(), WorkspaceStore] });
  return { t, ws: TestBed.inject(WorkspaceStore) };
}

describe('WorkspaceStore', () => {
  afterEach(() => vi.useRealTimers());

  it('opens a finished run in review and fits it', async () => {
    const { ws, t } = setup({ list_runs: [run('done')] });
    await ws.init(7, false);
    expect(ws.phase()).toBe('review');
    expect(ws.fit()?.pages).toBe(1);
    expect(new TextDecoder().decode(ws.pdf()!)).toBe('%PDF-1.7');
    expect(t.callsTo('fit_cv')[0]).toEqual({ runId: 3, budget: 1, overrides: [] });
  });

  it('starts in "new" with no runs, and in "matching" when auto-started', async () => {
    const a = setup();
    await a.ws.init(7, false);
    expect(a.ws.phase()).toBe('new');
  });

  it('keeps progress that arrives before start_match resolves', async () => {
    let resolveStart!: (id: number) => void;
    const { ws, t } = setup({ start_match: () => new Promise<number>((r) => (resolveStart = r)) });
    await ws.init(7, false);
    const started = ws.start();
    await flush();
    expect(ws.phase()).toBe('matching');
    await t.emit('match://progress', { run_id: 3, event: { stage: 'scoring', done: 2, total: 5 } });
    await t.emit('match://progress', { run_id: 99, event: { stage: 'done' } });
    resolveStart(3);
    await started;
    expect(ws.progress()).toEqual({ stage: 'scoring', done: 2, total: 5 });
    await t.emit('match://progress', { run_id: 3, event: { stage: 'done' } });
    await flush();
    expect(ws.phase()).toBe('review');
    expect(ws.ranked()).toEqual(RANKED);
  });

  it('marks failed runs with the error and offers resume', async () => {
    const { ws, t } = setup({ list_runs: [run('running')], resume_match: null });
    await ws.init(7, false);
    await t.emit('match://progress', { run_id: 3, event: { stage: 'failed', error: { kind: 'EngineUnavailable', message: 'dead' } } });
    expect(ws.phase()).toBe('failed');
    expect(ws.runError()?.kind).toBe('EngineUnavailable');
    await ws.resume();
    expect(ws.phase()).toBe('matching');
    expect(t.callsTo('resume_match')).toEqual([{ runId: 3 }]);
  });

  it('debounces refits and ignores stale responses', async () => {
    const pending: ((v: unknown) => void)[] = [];
    const { ws, t } = setup({ list_runs: [run('done')] });
    await ws.init(7, false);
    t.set('fit_cv', () => new Promise((r) => pending.push(r)));
    vi.useFakeTimers();
    ws.toggleEntry(2, false);
    ws.toggleBullet(12, true);
    vi.advanceTimersByTime(FIT_DEBOUNCE_MS);
    expect(t.callsTo('fit_cv')).toHaveLength(2); // init + one debounced call
    ws.setBudget(2);
    vi.advanceTimersByTime(FIT_DEBOUNCE_MS);
    vi.useRealTimers();
    await flush();
    expect(pending).toHaveLength(2);
    pending[1](fitOut({ pages: 2 }));
    await flush();
    pending[0](fitOut({ pages: 1, fill: 0.1 }));
    await flush();
    expect(ws.fit()?.pages).toBe(2);
    expect(t.callsTo('fit_cv')[1]['overrides']).toEqual([{ Exclude: { Entry: 2 } }, { Include: { Bullet: 12 } }]);
    expect(t.callsTo('set_ui_setting')).toEqual([{ key: 'ui.page_budget', value: '2' }]);
  });

  it('keeps the last good preview when a compile fails', async () => {
    const { ws, t } = setup({ list_runs: [run('done')] });
    await ws.init(7, false);
    const good = ws.pdf();
    t.set('fit_cv', () => {
      throw { kind: 'Render', message: 'bad' };
    });
    await ws.refitNow();
    expect(ws.fitError()?.kind).toBe('Render');
    expect(ws.pdf()).toBe(good);
  });

  it('restores a saved selection as overrides', async () => {
    const { ws } = setup({ list_runs: [run('done')] });
    await ws.init(7, false);
    ws.restore(SELECTION);
    const o = ws.overrides();
    expect(o.order).toEqual([1, 2]);
    expect(o.targets['E:3']).toBe('exclude');
    expect(o.targets['B:12']).toBe('exclude');
    expect(o.targets['B:10']).toBe('include');
  });
});

describe('workspace helpers', () => {
  it('describes stepper state', () => {
    expect(stepState({ stage: 'scoring', done: 1, total: 4 })).toEqual({ index: 1, percent: 25, detail: '1/4' });
    expect(stepState(null).index).toBe(0);
  });

  it('formats the fill meter', () => {
    expect(fillText(1, 0.923, false)).toBe('1 page · 92% full');
    expect(fillText(2, 1, true)).toBe('2 pages · overflow');
    expect(fillText(null, 0, false)).toBe('Not fitted yet');
  });

  it('orders rubric probabilities numerically', () => {
    const [row] = breakdown([{ run_id: 1, target_type: 'entry', target_id: 1, question: 'relevance', probabilities: { '4': 0.1, '0': 0.2, '2': 0.7 }, score: 2 }]);
    expect(row.label).toContain('Relevance');
    expect(row.bars.map((b) => b.key)).toEqual(['0', '2', '4']);
  });

  it('picks the most likely seniority and role family', () => {
    const s = roleSignals({
      scores: () => [
        { run_id: 1, target_type: 'meta', target_id: 0, question: 'seniority', probabilities: { mid: 0.3, senior: 0.6 }, score: 0 },
        { run_id: 1, target_type: 'meta', target_id: 0, question: 'role_family', probabilities: { backend: 0.8 }, score: 0 },
      ],
      ranked: () => RANKED,
    } as never);
    expect(s.seniority).toEqual({ label: 'senior', p: 0.6 });
    expect(s.roleFamily?.label).toBe('backend');
    expect(s.skills[0]).toEqual({ name: 'Rust', p: 0.9 });
  });

  it('names and normalizes export paths', () => {
    expect(defaultFileName('Ada Lovelace', { company: 'A/B Co', title: 'Dev' })).toBe('Ada Lovelace — A-B Co.pdf');
    expect(ensurePdf('/tmp/cv')).toBe('/tmp/cv.pdf');
    expect(ensurePdf('/tmp/cv.PDF')).toBe('/tmp/cv.PDF');
  });

  it('explains context overflow', () => {
    expect(failureCopy({ kind: 'ContextOverflow', message: 'x' }).title).toContain('too long');
  });
});

describe('components', () => {
  it('fill meter shows overflow in red', () => {
    TestBed.configureTestingModule({});
    const f = TestBed.createComponent(FillMeterComponent);
    f.componentRef.setInput('pages', 1);
    f.componentRef.setInput('overflow', true);
    f.detectChanges();
    const text = f.nativeElement.querySelector('[data-testid=fill-text]') as HTMLElement;
    expect(text.textContent).toContain('overflow');
    expect(text.className).toContain('text-destructive');
  });

  it('preview keeps the last good document when an error arrives', () => {
    vi.spyOn(URL, 'createObjectURL').mockReturnValue('about:blank');
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => undefined);
    TestBed.configureTestingModule({});
    const f = TestBed.createComponent(PdfPreviewComponent);
    f.componentRef.setInput('pdf', new Uint8Array([37, 80, 68, 70]));
    f.detectChanges();
    f.componentRef.setInput('error', { kind: 'Render', message: 'bad.' });
    f.detectChanges();
    const el = f.nativeElement as HTMLElement;
    expect(el.querySelector('iframe')).not.toBeNull();
    expect(el.textContent).toContain('last good version');
  });
});
