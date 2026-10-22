import { computed, inject, Injectable, signal } from '@angular/core';

import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import { onTablesChanged } from '../../core/ipc/events';
import type { FieldKey, FilterOp, JobDescription, JobFilter, JobStatus, JobView, JobViewInput, ViewLayout } from '../../core/ipc/types';

export type { FieldKey, FilterOp, JobFilter, JobSort, JobView, ViewLayout } from '../../core/ipc/types';

export const FIELDS: { key: FieldKey; label: string }[] = [
  { key: 'company', label: 'Company' },
  { key: 'title', label: 'Title' },
  { key: 'status', label: 'Status' },
  { key: 'url', label: 'Site' },
  { key: 'created', label: 'Created' },
];

export const OPS_BY_FIELD: Record<FieldKey, { op: FilterOp; label: string }[]> = {
  company: textOps(),
  title: textOps(),
  url: textOps(),
  status: [
    { op: 'is', label: 'is' },
    { op: 'is_not', label: 'is not' },
    { op: 'empty', label: 'is empty' },
    { op: 'not_empty', label: 'is not empty' },
  ],
  created: [
    { op: 'after', label: 'is after' },
    { op: 'before', label: 'is before' },
  ],
};

function textOps(): { op: FilterOp; label: string }[] {
  return [
    { op: 'contains', label: 'contains' },
    { op: 'not_contains', label: 'does not contain' },
    { op: 'empty', label: 'is empty' },
    { op: 'not_empty', label: 'is not empty' },
  ];
}

export const fieldLabel = (key: FieldKey) => FIELDS.find((f) => f.key === key)!.label;

export function newFilter(field: FieldKey): JobFilter {
  const op = OPS_BY_FIELD[field][0].op;
  return { field, op, value: field === 'status' ? [] : '' };
}

/** Whether a filter carries enough input to constrain anything. */
export function isActive(f: JobFilter): boolean {
  if (f.op === 'empty' || f.op === 'not_empty') return true;
  return Array.isArray(f.value) ? f.value.length > 0 : f.value.trim() !== '';
}

function textOf(j: JobDescription, field: FieldKey): string {
  if (field === 'company') return j.company;
  if (field === 'title') return j.title;
  if (field === 'url') return j.url;
  return j.created_at;
}

export function matches(j: JobDescription, f: JobFilter): boolean {
  if (!isActive(f)) return true;
  if (f.field === 'status') {
    const ids = f.value as number[];
    switch (f.op) {
      case 'is':
        return j.status_id !== null && ids.includes(j.status_id);
      case 'is_not':
        return j.status_id === null || !ids.includes(j.status_id);
      case 'empty':
        return j.status_id === null;
      default:
        return j.status_id !== null;
    }
  }
  const text = textOf(j, f.field).trim();
  const q = (f.value as string).trim().toLowerCase();
  switch (f.op) {
    case 'contains':
      return text.toLowerCase().includes(q);
    case 'not_contains':
      return !text.toLowerCase().includes(q);
    case 'empty':
      return !text;
    case 'not_empty':
      return !!text;
    case 'before':
      return text.slice(0, 10) < q;
    case 'after':
      return text.slice(0, 10) > q;
    default:
      return true;
  }
}

/** Filters (all must match) then sorts in priority order; ties keep the input order. */
export function applyView(jobs: JobDescription[], view: Pick<JobView, 'filters' | 'sorts'>, statuses: Map<number, JobStatus>): JobDescription[] {
  const kept = jobs.filter((j) => view.filters.every((f) => matches(j, f)));
  if (!view.sorts.length) return kept;
  const key = (j: JobDescription, field: FieldKey): string | number => {
    if (field === 'status') return j.status_id === null ? Number.MAX_SAFE_INTEGER : (statuses.get(j.status_id)?.position ?? 0);
    return textOf(j, field).toLowerCase();
  };
  return kept
    .map((j, i) => ({ j, i }))
    .sort((a, b) => {
      for (const s of view.sorts) {
        const x = key(a.j, s.field);
        const y = key(b.j, s.field);
        if (x !== y) return (x < y ? -1 : 1) * (s.dir === 'asc' ? 1 : -1);
      }
      return a.i - b.i;
    })
    .map(({ j }) => j);
}

/** The user's saved Jobs views, stored in the database. There is always at least one. */
@Injectable({ providedIn: 'root' })
export class JobViewsStore {
  private readonly toast = inject(ZardSonnerService);
  private loaded: Promise<void> | null = null;
  /** Pending saves per view, so edits land in order. */
  private readonly saving = new Map<number, Promise<unknown>>();

  readonly views = signal<JobView[]>([]);
  readonly activeId = signal<number | null>(null);
  readonly active = computed<JobView | null>(() => this.views().find((v) => v.id === this.activeId()) ?? this.views().at(0) ?? null);

  constructor() {
    // Skip while edits are in flight so a reload can't undo an optimistic change.
    void onTablesChanged(['job_view'], () => {
      if (this.loaded && this.saving.size === 0) void this.reload();
    }).catch(() => undefined);
  }

  /** Loads once; later calls reuse the first load. */
  ensureLoaded(): Promise<void> {
    return (this.loaded ??= this.reload());
  }

  async reload(): Promise<void> {
    try {
      this.views.set(await api.listJobViews());
    } catch (e) {
      this.toast.error('Could not load views', { description: messageFor(e) });
    }
  }

  async add(layout: ViewLayout): Promise<void> {
    await this.create({ name: layout === 'board' ? 'Board' : 'Table', layout, filters: [], sorts: [] });
  }

  async duplicate(id: number): Promise<void> {
    const src = this.views().find((v) => v.id === id);
    if (src) await this.create({ name: `${src.name} copy`, layout: src.layout, filters: src.filters, sorts: src.sorts });
  }

  /** Deletes a view unless it is the last one. */
  async remove(id: number): Promise<void> {
    if (this.views().length <= 1) return;
    try {
      await api.deleteJobView(id);
      this.views.update((vs) => vs.filter((v) => v.id !== id));
    } catch (e) {
      this.toast.error('Could not delete view', { description: messageFor(e) });
    }
  }

  /** Optimistic edit; reloads on failure. */
  patch(id: number, change: Partial<JobViewInput>): void {
    const current = this.views().find((v) => v.id === id);
    if (!current) return;
    const next = { ...current, ...change };
    this.views.update((vs) => vs.map((v) => (v.id === id ? next : v)));
    const input: JobViewInput = { name: next.name, layout: next.layout, filters: next.filters, sorts: next.sorts };
    const save = (this.saving.get(id) ?? Promise.resolve())
      .then(() => api.updateJobView(id, input))
      .catch(async (e) => {
        this.toast.error('Could not save view', { description: messageFor(e) });
        await this.reload();
      });
    this.saving.set(id, save);
  }

  private async create(input: JobViewInput): Promise<void> {
    try {
      const view = await api.createJobView(input);
      this.views.update((vs) => [...vs, view]);
      this.activeId.set(view.id);
    } catch (e) {
      this.toast.error('Could not create view', { description: messageFor(e) });
    }
  }
}
