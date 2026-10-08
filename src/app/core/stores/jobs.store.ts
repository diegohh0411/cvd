import { computed, Injectable, signal } from '@angular/core';

import * as api from '../ipc/commands';
import { messageFor } from '../ipc/errors';
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

  async create(title: string, company: string, rawText: string): Promise<number> {
    const id = await api.createJob(title, company, rawText);
    await this.load();
    return id;
  }

  setLatestRun(run: MatchRun): void {
    this.latestRuns.update((m) => ({ ...m, [run.jd_id]: run }));
  }
}

export function jobLabel(j: Pick<JobDescription, 'company' | 'title'>): string {
  return [j.company, j.title].filter((s) => s.trim()).join(' — ') || 'Untitled job';
}
