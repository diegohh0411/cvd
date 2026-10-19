import { computed, Injectable, signal } from '@angular/core';

import * as api from '../ipc/commands';
import { messageFor } from '../ipc/errors';
import type { Entry, EntryInput, EntryKind, Skill } from '../ipc/types';

@Injectable({ providedIn: 'root' })
export class LibraryStore {
  /** Every entry, archived included; views filter. */
  readonly entries = signal<Entry[]>([]);
  readonly skills = signal<Skill[]>([]);
  readonly loading = signal(false);
  readonly loaded = signal(false);
  readonly error = signal<string | null>(null);

  readonly active = computed(() => this.entries().filter((e) => !e.archived));
  readonly isEmpty = computed(() => this.loaded() && this.active().length === 0);

  byKind(kind: EntryKind, includeArchived = false): Entry[] {
    return this.entries().filter((e) => e.kind === kind && (includeArchived || !e.archived));
  }

  async load(): Promise<void> {
    this.loading.set(true);
    try {
      const [entries, skills] = await Promise.all([api.listEntries(true), api.listSkills()]);
      this.entries.set(entries);
      this.skills.set(skills);
      this.error.set(null);
    } catch (e) {
      this.error.set(messageFor(e));
    } finally {
      this.loading.set(false);
      this.loaded.set(true);
    }
  }

  async save(input: EntryInput): Promise<number> {
    const id = await api.saveEntry(input);
    await this.load();
    return id;
  }

  /** Stores the given display order as each entry's `sort_hint` (1-based) — the default CV order. */
  async setOrder(ordered: Entry[]): Promise<void> {
    try {
      for (const [i, e] of ordered.entries()) if (e.sort_hint !== i + 1) await api.saveEntry({ ...toInput(e), sort_hint: i + 1 });
    } finally {
      await this.load();
    }
  }

  async remove(id: number): Promise<void> {
    await api.deleteEntry(id);
    await this.load();
  }
}

/** The writable shape of an existing entry. */
export function toInput(e: Entry): EntryInput {
  return {
    id: e.id,
    kind: e.kind,
    title: e.title,
    org: e.org,
    location: e.location,
    start_date: e.start_date,
    end_date: e.end_date,
    description: e.description,
    sort_hint: e.sort_hint,
    archived: e.archived,
    bullets: [...e.bullets].sort((a, b) => a.position - b.position).map((b) => b.text),
    skills: e.skills.map((s) => s.name),
  };
}

export function emptyInput(kind: EntryKind): EntryInput {
  return {
    id: null,
    kind,
    title: '',
    org: '',
    location: '',
    start_date: '',
    end_date: null,
    description: '',
    sort_hint: 0,
    archived: false,
    bullets: [],
    skills: [],
  };
}
