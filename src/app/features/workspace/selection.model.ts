// Pure view-model logic for the review list: merges the ranking, the fitter's
// current selection and the user's overrides. No Angular, so it is unit-tested directly.
import type { EntryKind, Override, Ranked, Selection, Target } from '../../core/ipc/types';

/** Mirrors `MatchConfig::default().score_floor` in src-tauri/src/matcher/mod.rs. */
export const SCORE_FLOOR = 1.5;
export const SCORE_MAX = 4;

export type Section = 'Experience' | 'Projects' | 'Education';

/** Mirrors `section_of` in src-tauri/src/render/fit.rs. */
export function sectionOf(kind: EntryKind): Section {
  return kind === 'project' ? 'Projects' : kind === 'education' ? 'Education' : 'Experience';
}

export interface BulletView {
  id: number;
  text: string;
  score: number;
  included: boolean;
  belowFloor: boolean;
  /** The user asked for it but the fitter could not keep it. */
  dropped: boolean;
  /** Unchecking would leave its entry with no bullets. */
  locked: boolean;
}

export interface EntryView {
  id: number;
  title: string;
  org: string;
  kind: EntryKind;
  score: number;
  included: boolean;
  pinned: boolean;
  belowFloor: boolean;
  dropped: boolean;
  bullets: BulletView[];
}

export interface SectionView {
  section: Section;
  entries: EntryView[];
}

/** User overrides as a map, so a later choice for a target replaces the earlier one. */
export interface OverrideState {
  targets: Record<string, 'include' | 'exclude'>;
  order: number[] | null;
  /** Bullet ids in render order within their entry; absent = score order. */
  bulletOrder?: number[] | null;
}

export const NO_OVERRIDES: OverrideState = { targets: {}, order: null };

const key = (t: Target) => ('Entry' in t ? `E:${t.Entry}` : `B:${t.Bullet}`);
const target = (k: string): Target => (k.startsWith('E:') ? { Entry: Number(k.slice(2)) } : { Bullet: Number(k.slice(2)) });

export function toOverrides(s: OverrideState): Override[] {
  const out: Override[] = Object.entries(s.targets).map(([k, v]) => (v === 'include' ? { Include: target(k) } : { Exclude: target(k) }));
  if (s.order) out.push({ Order: s.order });
  if (s.bulletOrder?.length) out.push({ BulletOrder: s.bulletOrder });
  return out;
}

function set(s: OverrideState, t: Target, v: 'include' | 'exclude' | null): OverrideState {
  const targets = { ...s.targets };
  if (v) targets[key(t)] = v;
  else delete targets[key(t)];
  return { ...s, targets };
}

/**
 * Pins a selection as explicit overrides so later refits keep exactly it: the fitter
 * stops filling freed space on its own and only the user's edits change the CV.
 */
export function freeze(
  ranked: Ranked,
  selection: Selection,
  order: number[] | null = selection.entries.map((e) => e.entry_id),
  bulletOrder: number[] | null = null,
): OverrideState {
  const chosen = new Map(selection.entries.map((e) => [e.entry_id, new Set(e.bullet_ids)]));
  const targets: OverrideState['targets'] = {};
  for (const re of ranked.entries) {
    const bullets = chosen.get(re.entry.id);
    if (!bullets) targets[key({ Entry: re.entry.id })] = 'exclude';
    for (const [b] of re.bullets) targets[key({ Bullet: b.id })] = bullets?.has(b.id) ? 'include' : 'exclude';
  }
  return { targets, order, bulletOrder };
}

/** Checks or unchecks a whole entry. */
export function toggleEntry(s: OverrideState, ranked: Ranked, entryId: number, include: boolean): OverrideState {
  let next = set(s, { Entry: entryId }, include ? 'include' : 'exclude');
  const re = ranked.entries.find((r) => r.entry.id === entryId);
  if (include && re) {
    // Let the fitter pick bullets for a newly checked entry, but only if none are pinned yet.
    if (!re.bullets.some(([b]) => next.targets[key({ Bullet: b.id })] === 'include'))
      for (const [b] of re.bullets) next = set(next, { Bullet: b.id }, null);
  }
  if (!include && re) {
    // Pinned bullets would otherwise keep nothing; drop them with the entry.
    for (const [b] of re.bullets) if (next.targets[key({ Bullet: b.id })] === 'include') next = set(next, { Bullet: b.id }, null);
  }
  return next;
}

/** Checks or unchecks one bullet; checking also un-excludes its entry. */
export function toggleBullet(s: OverrideState, ranked: Ranked, bulletId: number, include: boolean): OverrideState {
  let next = set(s, { Bullet: bulletId }, include ? 'include' : 'exclude');
  if (include) {
    const re = ranked.entries.find((r) => r.bullets.some(([b]) => b.id === bulletId));
    if (re && next.targets[key({ Entry: re.entry.id })] === 'exclude') next = set(next, { Entry: re.entry.id }, null);
  }
  return next;
}

export function reorder(s: OverrideState, entryIds: number[]): OverrideState {
  return { ...s, order: entryIds };
}

/** Moves a bullet within its entry's displayed list; the DB position is untouched. */
export function reorderBullets(s: OverrideState, view: SectionView[], entryId: number, from: number, to: number): OverrideState | null {
  const entry = view.flatMap((g) => g.entries).find((e) => e.id === entryId);
  if (!entry || from === to || from < 0 || from >= entry.bullets.length) return null;
  const ids = entry.bullets.map((b) => b.id);
  ids.splice(Math.max(0, Math.min(ids.length - 1, to)), 0, ...ids.splice(from, 1));
  const others = view.flatMap((g) => g.entries).filter((e) => e.id !== entryId).flatMap((e) => e.bullets.map((b) => b.id));
  return { ...s, bulletOrder: [...others, ...ids] };
}

/** Grouped, render-ordered view: selected entries in selection order, then the rest by score. */
export function buildView(ranked: Ranked, selection: Selection | null, s: OverrideState): SectionView[] {
  const bpos = new Map((s.bulletOrder ?? []).map((id, i) => [id, i]));
  const chosen = new Map((selection?.entries ?? []).map((e, i) => [e.entry_id, { ...e, i }]));
  const sections: Record<Section, EntryView[]> = { Experience: [], Projects: [], Education: [] };
  for (const re of ranked.entries) {
    const sel = chosen.get(re.entry.id);
    const wantEntry = s.targets[key({ Entry: re.entry.id })] === 'include';
    const selectedBullets = new Set(sel?.bullet_ids ?? []);
    const bullets: BulletView[] = [...re.bullets]
      .sort((a, b) => (bpos.get(a[0].id) ?? Number.MAX_SAFE_INTEGER) - (bpos.get(b[0].id) ?? Number.MAX_SAFE_INTEGER) || b[1] - a[1] || a[0].position - b[0].position || a[0].id - b[0].id)
      .map(([b, score]) => ({
        id: b.id,
        text: b.text,
        score,
        included: selectedBullets.has(b.id),
        belowFloor: score < SCORE_FLOOR,
        dropped: !selectedBullets.has(b.id) && s.targets[key({ Bullet: b.id })] === 'include',
        locked: selectedBullets.has(b.id) && selectedBullets.size === 1,
      }));
    sections[sectionOf(re.entry.kind)].push({
      id: re.entry.id,
      title: re.entry.title,
      org: re.entry.org,
      kind: re.entry.kind,
      score: re.score,
      included: !!sel,
      pinned: !!sel?.pinned,
      belowFloor: re.score < SCORE_FLOOR,
      dropped: !sel && (wantEntry || bullets.some((b) => b.dropped)),
      bullets,
    });
  }
  const pos = (e: EntryView) => chosen.get(e.id)?.i ?? Number.MAX_SAFE_INTEGER;
  return (Object.keys(sections) as Section[])
    .map((section) => ({
      section,
      entries: sections[section].sort((a, b) => pos(a) - pos(b) || b.score - a.score),
    }))
    .filter((g) => g.entries.length);
}

/** All selected entry ids in render order after moving `entryId` within its section. */
export function moveEntry(view: SectionView[], entryId: number, toIndex: number): number[] | null {
  const group = view.find((g) => g.entries.some((e) => e.id === entryId));
  if (!group) return null;
  const selected = group.entries.filter((e) => e.included);
  const from = selected.findIndex((e) => e.id === entryId);
  if (from < 0) return null;
  const to = Math.max(0, Math.min(selected.length - 1, toIndex));
  if (from === to) return null;
  const ids = selected.map((e) => e.id);
  ids.splice(to, 0, ...ids.splice(from, 1));
  return view.flatMap((g) => (g === group ? ids : g.entries.filter((e) => e.included).map((e) => e.id)));
}

export interface SelectionCounts {
  entries: number;
  totalEntries: number;
  bullets: number;
  totalBullets: number;
}

export function counts(ranked: Ranked, selection: Selection | null): SelectionCounts {
  return {
    entries: selection?.entries.length ?? 0,
    totalEntries: ranked.entries.length,
    bullets: selection?.entries.reduce((n, e) => n + e.bullet_ids.length, 0) ?? 0,
    totalBullets: ranked.entries.reduce((n, e) => n + e.bullets.length, 0),
  };
}
