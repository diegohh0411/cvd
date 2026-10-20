import { computed, Injectable, signal } from '@angular/core';

import * as api from '../ipc/commands';
import { messageFor } from '../ipc/errors';
import { onJobUpdated } from '../ipc/events';
import type { JobDescription, MatchRun } from '../ipc/types';

export const RECENT_JOBS = 5;

@Injectable({ providedIn: 'root' })
export class JobsStore {
  readonly jobs = signal<JobDescription[]>([]);
  /** Latest run per job id, when the job has one. */
  readonly latestRuns = signal<Record<number, MatchRun | undefined>>({});
  readonly loading = signal(false);
  readonly error = signal<string | null>(null);

  readonly sorted = computed(() => [...this.jobs()].sort((a, b) => b.id - a.id));
  readonly recent = computed(() => this.sorted().slice(0, RECENT_JOBS));

  constructor() {
    // Auto-apply marks jobs Applied from its own window.
    void onJobUpdated(({ job_id, status_id }) =>
      this.jobs.update((js) => js.map((j) => (j.id === job_id ? { ...j, status_id } : j))),
    ).catch(() => undefined);
  }

  async load(): Promise<void> {
    this.loading.set(true);
    try {
      const jobs = await api.listJobs();
      this.jobs.set(jobs);
      const runs = await Promise.all(jobs.map((j) => api.listRuns(j.id).then((r) => [j.id, r[0]] as const)));
      this.latestRuns.set(Object.fromEntries(runs));
      this.error.set(null);
    } catch (e) {
      this.error.set(messageFor(e));
    } finally {
      this.loading.set(false);
    }
  }

  async create(title: string, company: string, rawText: string, url = '', statusId: number | null = null): Promise<number> {
    const id = await api.createJob(title, company, rawText, url, statusId);
    await this.load();
    return id;
  }

  /** Saves status/URL and patches the cached job so lists update at once. */
  async setTracking(id: number, statusId: number | null, url: string): Promise<void> {
    await api.updateJobTracking(id, statusId, url);
    this.jobs.update((js) => js.map((j) => (j.id === id ? { ...j, status_id: statusId, url: url.trim() } : j)));
  }

  /** Saves the description text and patches the cached job. */
  async setDescription(id: number, rawText: string): Promise<void> {
    await api.updateJobDescription(id, rawText);
    this.jobs.update((js) => js.map((j) => (j.id === id ? { ...j, raw_text: rawText } : j)));
  }

  /** A status was deleted server-side (ON DELETE SET NULL); mirror it. */
  clearStatus(statusId: number): void {
    this.jobs.update((js) => js.map((j) => (j.status_id === statusId ? { ...j, status_id: null } : j)));
  }

  setLatestRun(run: MatchRun): void {
    this.latestRuns.update((m) => ({ ...m, [run.jd_id]: run }));
  }
}

export function jobLabel(j: Pick<JobDescription, 'company' | 'title'>): string {
  return [j.company, j.title].filter((s) => s.trim()).join(' — ') || 'Untitled job';
}
