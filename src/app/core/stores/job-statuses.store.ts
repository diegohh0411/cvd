import { computed, inject, Injectable, signal } from '@angular/core';

import * as api from '../ipc/commands';
import { messageFor } from '../ipc/errors';
import { STATUS_COLORS, type JobStatus, type StatusColor } from '../ipc/types';
import { JobsStore } from './jobs.store';

/** The user's application status labels (Bookmarked, Applied, ...), shared app-wide. */
@Injectable({ providedIn: 'root' })
export class JobStatusesStore {
  private readonly jobs = inject(JobsStore);
  private loaded: Promise<void> | null = null;

  readonly statuses = signal<JobStatus[]>([]);
  readonly error = signal<string | null>(null);
  readonly byId = computed(() => new Map(this.statuses().map((s) => [s.id, s])));

  /** Loads once; later calls reuse the first load. */
  ensureLoaded(): Promise<void> {
    return (this.loaded ??= this.reload());
  }

  async reload(): Promise<void> {
    try {
      this.statuses.set(await api.listJobStatuses());
      this.error.set(null);
    } catch (e) {
      this.error.set(messageFor(e));
    }
  }

  /** Next palette color not yet in use, so new labels look distinct. */
  nextColor(): StatusColor {
    const used = new Set(this.statuses().map((s) => s.color));
    return STATUS_COLORS.find((c) => c !== 'gray' && !used.has(c)) ?? 'gray';
  }

  async create(name: string, color: StatusColor = this.nextColor()): Promise<JobStatus> {
    const created = await api.createJobStatus(name, color);
    this.statuses.update((ss) => [...ss, created]);
    return created;
  }

  async update(id: number, name: string, color: StatusColor): Promise<void> {
    const saved = await api.updateJobStatus(id, name, color);
    this.statuses.update((ss) => ss.map((s) => (s.id === id ? saved : s)));
  }

  async remove(id: number): Promise<void> {
    await api.deleteJobStatus(id);
    this.statuses.update((ss) => ss.filter((s) => s.id !== id));
    this.jobs.clearStatus(id);
  }

  /** Optimistic reorder; reloads on failure. */
  async reorder(ids: number[]): Promise<void> {
    const by = this.byId();
    this.statuses.set(ids.map((id, position) => ({ ...by.get(id)!, position })));
    try {
      await api.reorderJobStatuses(ids);
    } catch (e) {
      await this.reload();
      throw e;
    }
  }
}
