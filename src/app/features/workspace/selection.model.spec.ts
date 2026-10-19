import { RANKED, SELECTION } from './fixtures';
import { buildView, counts, moveEntry, NO_OVERRIDES, reorder, reorderBullets, sectionOf, toggleBullet, toggleEntry, toOverrides } from './selection.model';

describe('selection model', () => {
  it('maps kinds to the fitter sections', () => {
    expect(sectionOf('volunteer')).toBe('Experience');
    expect(sectionOf('project')).toBe('Projects');
  });

  it('builds sections with selected entries first and bullets by score', () => {
    const view = buildView(RANKED, SELECTION, NO_OVERRIDES);
    expect(view.map((g) => g.section)).toEqual(['Experience', 'Projects']);
    const [e1, e2] = view[0].entries;
    expect([e1.id, e1.included, e2.id, e2.included]).toEqual([1, true, 2, true]);
    expect(e1.bullets.map((b) => [b.id, b.included])).toEqual([
      [10, true],
      [11, true],
      [12, false],
    ]);
    expect(e1.bullets[2].belowFloor).toBe(true);
    expect(view[1].entries[0].belowFloor).toBe(true);
  });

  it('locks the last selected bullet of an entry', () => {
    const view = buildView(RANKED, SELECTION, NO_OVERRIDES);
    expect(view[0].entries[1].bullets[0]).toMatchObject({ id: 20, locked: true });
    expect(view[0].entries[0].bullets[0].locked).toBe(false);
  });

  it('turns toggles into serde-shaped overrides, latest choice per target wins', () => {
    let s = toggleBullet(NO_OVERRIDES, RANKED, 12, true);
    s = toggleBullet(s, RANKED, 12, false);
    s = toggleEntry(s, RANKED, 3, true);
    s = reorder(s, [2, 1]);
    expect(toOverrides(s)).toEqual([{ Exclude: { Bullet: 12 } }, { Include: { Entry: 3 } }, { Order: [2, 1] }]);
  });

  it('excluding an entry drops its pinned bullets; including a bullet un-excludes its entry', () => {
    let s = toggleBullet(NO_OVERRIDES, RANKED, 11, true);
    s = toggleEntry(s, RANKED, 1, false);
    expect(toOverrides(s)).toEqual([{ Exclude: { Entry: 1 } }]);
    s = toggleBullet(s, RANKED, 12, true);
    expect(toOverrides(s)).toEqual([{ Include: { Bullet: 12 } }]);
  });

  it('marks items the user asked for but the fit dropped', () => {
    const s = toggleEntry(NO_OVERRIDES, RANKED, 3, true);
    const view = buildView(RANKED, SELECTION, s);
    expect(view[1].entries[0].dropped).toBe(true);
  });

  it('moves a selected entry within its section', () => {
    const view = buildView(RANKED, SELECTION, NO_OVERRIDES);
    expect(moveEntry(view, 2, 0)).toEqual([2, 1]);
    expect(moveEntry(view, 1, 0)).toBeNull();
    expect(moveEntry(view, 3, 0)).toBeNull();
  });

  it('counts selected and total items', () => {
    expect(counts(RANKED, SELECTION)).toEqual({ entries: 2, totalEntries: 3, bullets: 3, totalBullets: 6 });
  });

  it('lists bullets in DB order and reorders them without touching positions', () => {
    const view = buildView(RANKED, SELECTION, NO_OVERRIDES);
    const entry = view.flatMap((g) => g.entries).find((e) => e.bullets.length > 1)!;
    const dbOrder = entry.bullets.map((b) => b.id);
    const next = reorderBullets(NO_OVERRIDES, view, entry.id, 0, 1)!;
    const moved = buildView(RANKED, SELECTION, next).flatMap((g) => g.entries).find((e) => e.id === entry.id)!;
    expect(moved.bullets.map((b) => b.id)).toEqual([dbOrder[1], dbOrder[0], ...dbOrder.slice(2)]);
    expect(toOverrides(next)).toContainEqual({ BulletOrder: next.bulletOrder });
  });
});
