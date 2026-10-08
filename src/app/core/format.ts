import type { EntryKind } from './ipc/types';

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/** `2023-03` → `Mar 2023`; anything else is returned unchanged. */
export function formatMonth(ym: string | null | undefined): string {
  const m = /^(\d{4})-(\d{2})$/.exec(ym ?? '');
  if (!m) return ym ?? '';
  const month = Number(m[2]);
  return month >= 1 && month <= 12 ? `${MONTHS[month - 1]} ${m[1]}` : ym!;
}

export function formatRange(start: string, end: string | null): string {
  const s = formatMonth(start);
  const e = end ? formatMonth(end) : 'Present';
  return s ? `${s} – ${e}` : e === 'Present' ? '' : e;
}

/** SQLite `CURRENT_TIMESTAMP` (UTC, `YYYY-MM-DD HH:MM:SS`) → local short date. */
export function formatTimestamp(ts: string): string {
  const d = new Date(ts.includes('T') ? ts : `${ts.replace(' ', 'T')}Z`);
  return Number.isNaN(d.getTime()) ? ts : d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
}

export const KIND_LABELS: Record<EntryKind, { one: string; many: string }> = {
  experience: { one: 'Experience', many: 'Experience' },
  project: { one: 'Project', many: 'Projects' },
  education: { one: 'Education', many: 'Education' },
  volunteer: { one: 'Volunteer role', many: 'Volunteer' },
};

export function isEntryKind(k: string): k is EntryKind {
  return k in KIND_LABELS;
}
