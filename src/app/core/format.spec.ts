import { formatMonth, formatRange, isEntryKind } from './format';

describe('format', () => {
  it('formats months and ranges', () => {
    expect(formatMonth('2023-03')).toBe('Mar 2023');
    expect(formatMonth('2023')).toBe('2023');
    expect(formatRange('2021-01', '2023-12')).toBe('Jan 2021 – Dec 2023');
    expect(formatRange('2021-01', null)).toBe('Jan 2021 – Present');
    expect(formatRange('', null)).toBe('');
  });

  it('recognizes entry kinds', () => {
    expect(isEntryKind('project')).toBe(true);
    expect(isEntryKind('skills')).toBe(false);
  });
});
