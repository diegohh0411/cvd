import type { Bullet, Entry, EntryKind, FitOutput, Ranked, Selection } from '../../core/ipc/types';

export function rankedEntry(id: number, score: number, bullets: [number, number][], kind: EntryKind = 'experience') {
  const entry: Entry = {
    id,
    kind,
    title: `Entry ${id}`,
    org: 'Org',
    location: '',
    start_date: '2020-01',
    end_date: null,
    description: '',
    sort_hint: 0,
    archived: false,
    bullets: [],
    skills: [],
  };
  const bs: [Bullet, number][] = bullets.map(([bid, s], i) => [{ id: bid, entry_id: id, text: `Bullet ${bid}`, position: i }, s]);
  return { entry: { ...entry, bullets: bs.map(([b]) => b) }, score, bullets: bs };
}

export const RANKED: Ranked = {
  entries: [
    rankedEntry(1, 3.6, [
      [10, 3.5],
      [11, 2.0],
      [12, 1.0],
    ]),
    rankedEntry(2, 2.4, [
      [20, 2.5],
      [21, 1.2],
    ]),
    rankedEntry(3, 1.0, [[30, 1.0]], 'project'),
  ],
  skills: [
    [{ id: 1, name: 'Rust', category: '' }, 0.9],
    [{ id: 2, name: 'Go', category: '' }, 0.2],
  ],
  seniority: 'senior',
};

export const SELECTION: Selection = {
  entries: [
    { entry_id: 1, bullet_ids: [10, 11], pinned: false },
    { entry_id: 2, bullet_ids: [20], pinned: false },
  ],
  skills: ['Rust'],
};

export function fitOut(p: Partial<FitOutput> = {}): FitOutput {
  return { selection: SELECTION, pages: 1, fill: 0.92, overflow: false, pdf_base64: btoa('%PDF-1.7'), ...p };
}
