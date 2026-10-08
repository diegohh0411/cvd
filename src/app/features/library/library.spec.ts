import { TestBed } from '@angular/core/testing';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri } from '../../../testing/tauri-mock';
import type { Entry, EntryInput } from '../../core/ipc/types';
import { emptyInput, LibraryStore, toInput } from '../../core/stores/library.store';
import { moveBy } from './bullet-list-editor.component';
import { cleanInput, entryErrors } from './entry-form.component';
import { LibraryPageComponent } from './library-page.component';
import { skillOptions } from './skill-input.component';
import { groupSkills } from './skills-page.component';

const skill = (id: number, name: string, category = '') => ({ id, name, category });
const entry = (p: Partial<Entry> = {}): Entry => ({
  id: 1,
  kind: 'experience',
  title: 'Engineer',
  org: 'Acme',
  location: '',
  start_date: '2021-01',
  end_date: null,
  description: '',
  sort_hint: 0,
  archived: false,
  bullets: [
    { id: 11, entry_id: 1, text: 'second', position: 1 },
    { id: 10, entry_id: 1, text: 'first', position: 0 },
  ],
  skills: [skill(1, 'Rust')],
  ...p,
});

describe('entry editing helpers', () => {
  it('maps an entry to its input with bullets in position order', () => {
    const input = toInput(entry());
    expect(input.bullets).toEqual(['first', 'second']);
    expect(input.skills).toEqual(['Rust']);
    expect(input.id).toBe(1);
  });

  it('validates required fields and date order', () => {
    const d: EntryInput = { ...emptyInput('project') };
    expect(entryErrors(d)).toEqual(['Title is required.', 'Start date is required.']);
    expect(entryErrors({ ...d, title: 'X', start_date: '2022-05', end_date: '2021-01' })).toEqual([
      'End date is before the start date.',
    ]);
    expect(entryErrors({ ...d, title: 'X', start_date: '2022-05' })).toEqual([]);
  });

  it('drops empty bullets and trims text', () => {
    const c = cleanInput({ ...emptyInput('experience'), title: ' T ', bullets: [' a ', '', '  '] });
    expect(c.title).toBe('T');
    expect(c.bullets).toEqual(['a']);
  });

  it('moves bullets by one and clamps at the ends', () => {
    const l = ['a', 'b', 'c'];
    expect(moveBy(l, 0, 1)).toBe(1);
    expect(l).toEqual(['b', 'a', 'c']);
    expect(moveBy(l, 0, -1)).toBe(0);
    expect(l).toEqual(['b', 'a', 'c']);
  });

  it('suggests skills case-insensitively and offers to create unknown ones', () => {
    const all = [skill(1, 'Rust'), skill(2, 'TypeScript'), skill(3, 'Rails')];
    expect(skillOptions('r', all, ['Rails'])).toEqual([
      { label: 'Rust', create: false },
      { label: 'TypeScript', create: false },
      { label: 'r', create: true },
    ]);
    expect(skillOptions('rust', all, [])).toEqual([{ label: 'Rust', create: false }]);
    expect(skillOptions('  ', all, [])).toEqual([]);
    expect(skillOptions('Go', all, [])).toEqual([{ label: 'Go', create: true }]);
  });

  it('groups skills by category with usage counts', () => {
    const groups = groupSkills([skill(1, 'Rust', 'Languages'), skill(2, 'Docker')], [entry(), entry({ id: 2, archived: true })]);
    expect(groups.map((g) => g.category)).toEqual(['Languages', 'Uncategorized']);
    expect(groups[0].skills[0].uses).toBe(1);
  });
});

describe('LibraryPageComponent', () => {
  async function render(entries: Entry[]) {
    mockTauri({ list_entries: entries, list_skills: [] });
    TestBed.configureTestingModule({ providers: pageProviders() });
    await TestBed.inject(LibraryStore).load();
    const f = TestBed.createComponent(LibraryPageComponent);
    f.componentRef.setInput('kind', 'experience');
    f.detectChanges();
    await flush();
    f.detectChanges();
    return f.nativeElement as HTMLElement;
  }

  it('shows the empty state with a call to action', async () => {
    const el = await render([]);
    expect(el.textContent).toContain('No experience yet');
    expect(el.textContent).toContain('New experience');
  });

  it('lists entries with formatted dates and hides archived ones', async () => {
    const el = await render([entry(), entry({ id: 2, title: 'Old job', archived: true })]);
    expect(el.textContent).toContain('Engineer');
    expect(el.textContent).toContain('Jan 2021 – Present');
    expect(el.textContent).not.toContain('Old job');
  });
});

describe('month field conversion', () => {
  it('round-trips YYYY-MM through a local Date', async () => {
    const { monthToDate, dateToMonth } = await import('./month-field.component');
    const d = monthToDate('2023-03')!;
    expect([d.getFullYear(), d.getMonth(), d.getDate()]).toEqual([2023, 2, 1]);
    expect(dateToMonth(new Date(2024, 10, 17))).toBe('2024-11');
    expect(monthToDate('')).toBeNull();
    expect(monthToDate(null)).toBeNull();
  });
});
