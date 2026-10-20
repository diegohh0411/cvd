import type { JobDescription, JobStatus } from '../../core/ipc/types';
import { applyView, type JobView, matches } from './job-views';

const job = (id: number, company: string, status_id: number | null, created_at = '2026-10-01 10:00:00'): JobDescription => ({
  id,
  title: 'Engineer',
  company,
  raw_text: '',
  created_at,
  url: '',
  status_id,
  company_id: null,
});

const STATUSES = new Map<number, JobStatus>([
  [1, { id: 1, name: 'Applied', color: 'blue', position: 1 } as JobStatus],
  [2, { id: 2, name: 'Bookmarked', color: 'gray', position: 0 } as JobStatus],
]);

const view = (v: Partial<JobView>): JobView => ({ id: 1, name: 'V', layout: 'table', filters: [], sorts: [], position: 0, ...v });

describe('job views', () => {
  const jobs = [job(1, 'Acme', 1), job(2, 'Globex', 2, '2026-09-01 10:00:00'), job(3, '', null)];

  it('filters by status, ignoring filters without a value', () => {
    expect(applyView(jobs, view({ filters: [{ field: 'status', op: 'is', value: [2] }] }), STATUSES).map((j) => j.id)).toEqual([2]);
    expect(applyView(jobs, view({ filters: [{ field: 'status', op: 'is', value: [] }] }), STATUSES)).toHaveLength(3);
    expect(applyView(jobs, view({ filters: [{ field: 'status', op: 'empty', value: [] }] }), STATUSES).map((j) => j.id)).toEqual([3]);
  });

  it('matches text and dates', () => {
    expect(matches(jobs[0], { field: 'company', op: 'contains', value: 'ac' })).toBe(true);
    expect(matches(jobs[2], { field: 'company', op: 'not_empty', value: '' })).toBe(false);
    expect(matches(jobs[1], { field: 'created', op: 'before', value: '2026-09-15' })).toBe(true);
  });

  it('sorts by status order then company, nulls last', () => {
    const v = view({ sorts: [{ field: 'status', dir: 'asc' }, { field: 'company', dir: 'desc' }] });
    expect(applyView(jobs, v, STATUSES).map((j) => j.id)).toEqual([2, 1, 3]);
  });
});
