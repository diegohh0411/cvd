import { mockTauri } from '../../../testing/tauri-mock';
import * as api from './commands';
import { isAppError } from './errors';
import { onMatchProgress } from './events';
import type { MatchStage, Override } from './types';

describe('ipc commands', () => {
  it('passes camelCase argument names that Tauri maps to snake_case', async () => {
    const t = mockTauri({
      list_entries: [],
      create_job: 7,
      start_match: 3,
      fit_cv: { selection: { entries: [], skills: [] }, pages: 1, fill: 0.5, overflow: false, pdf_base64: '' },
      export_cv: 1,
      list_runs: [],
      list_cvs: [],
      get_scores: [],
    });
    await api.listEntries(true);
    await api.createJob('Dev', 'Co', 'text');
    await api.startMatch(7);
    await api.fitCv(3, 1, []);
    await api.exportCv(3, { entries: [], skills: [] }, '/tmp/cv.pdf');
    await api.listRuns(7);
    await api.listCvs(7);
    await api.getScores(3);
    expect(t.calls.map((c) => [c.cmd, Object.keys(c.args).sort()])).toEqual([
      ['list_entries', ['includeArchived']],
      ['create_job', ['company', 'rawText', 'title']],
      ['start_match', ['jdId']],
      ['fit_cv', ['budget', 'overrides', 'runId']],
      ['export_cv', ['path', 'runId', 'selection']],
      ['list_runs', ['jdId']],
      ['list_cvs', ['jdId']],
      ['get_scores', ['runId']],
    ]);
  });

  it('serializes overrides as serde externally tagged enums', async () => {
    const t = mockTauri({ fit_cv: {} });
    const overrides: Override[] = [{ Include: { Entry: 5 } }, { Exclude: { Bullet: 9 } }, { Order: [3, 1, 2] }];
    await api.fitCv(1, 2, overrides);
    expect(JSON.stringify(t.callsTo('fit_cv')[0]['overrides'])).toBe(
      '[{"Include":{"Entry":5}},{"Exclude":{"Bullet":9}},{"Order":[3,1,2]}]',
    );
  });

  it('normalizes rejections into AppError', async () => {
    mockTauri({
      start_match: () => {
        throw { kind: 'ModelMissing', message: 'No model.' };
      },
      list_jobs: () => {
        throw new Error('boom');
      },
    });
    await expect(api.startMatch(1)).rejects.toEqual({ kind: 'ModelMissing', message: 'No model.' });
    const err = await api.listJobs().catch((e: unknown) => e);
    expect(isAppError(err)).toBe(true);
  });

  it('filters match progress by run id', async () => {
    const t = mockTauri();
    const seen: MatchStage[] = [];
    await onMatchProgress(2, (e) => seen.push(e));
    await t.emit('match://progress', { run_id: 1, event: { stage: 'reading_jd' } });
    await t.emit('match://progress', { run_id: 2, event: { stage: 'done' } });
    expect(seen).toEqual([{ stage: 'done' }]);
  });
});
