import { CdkDrag, CdkDragDrop, CdkDragHandle, CdkDropList } from '@angular/cdk/drag-drop';
import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucideGripVertical, lucidePin } from '@ng-icons/lucide';

import { ZardBadgeComponent } from '@/shared/components/badge/badge.component';
import type { ScoreRow } from '../../core/ipc/types';
import { ScoreBarComponent } from './score-bar.component';
import { type EntryView, moveEntry } from './selection.model';
import { WorkspaceStore } from './workspace.store';

@Component({
  selector: 'app-selection-list',
  imports: [CdkDropList, CdkDrag, CdkDragHandle, NgIcon, ZardBadgeComponent, ScoreBarComponent],
  viewProviders: [provideIcons({ lucideGripVertical, lucidePin })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <p class="sr-only" aria-live="polite">{{ live() }}</p>
    @for (group of ws.view(); track group.section) {
      <section class="mb-4">
        <h3 class="text-muted-foreground mb-2 text-xs font-medium tracking-wide uppercase">{{ group.section }}</h3>
        <ul cdkDropList (cdkDropListDropped)="drop($event, group.entries)" class="flex flex-col gap-2">
          @for (e of group.entries; track e.id; let i = $index) {
            <li
              cdkDrag
              [cdkDragDisabled]="!e.included"
              class="bg-card rounded-lg border p-3"
              [class.opacity-60]="!e.included"
              [attr.data-testid]="'entry-' + e.id"
            >
              <div class="flex items-center gap-2">
                <button
                  type="button"
                  cdkDragHandle
                  class="text-muted-foreground hover:text-foreground focus-visible:text-foreground disabled:opacity-30"
                  [disabled]="!e.included"
                  [attr.aria-label]="'Reorder ' + e.title + ' (Alt+Up/Down)'"
                  (keydown)="key($event, e)"
                >
                  <ng-icon name="lucideGripVertical" class="size-4" />
                </button>
                <input
                  type="checkbox"
                  class="accent-primary size-4"
                  [checked]="e.included"
                  [attr.aria-label]="'Include ' + e.title"
                  (change)="ws.toggleEntry(e.id, $any($event.target).checked)"
                />
                <div class="min-w-0 flex-1">
                  <span class="font-medium">{{ e.title }}</span>
                  @if (e.org) {
                    <span class="text-muted-foreground"> · {{ e.org }}</span>
                  }
                </div>
                @if (e.pinned) {
                  <ng-icon name="lucidePin" class="text-muted-foreground size-3.5" aria-label="Pinned by you" />
                }
                @if (e.dropped) {
                  <z-badge zType="outline" class="text-amber-600">dropped to fit</z-badge>
                } @else if (e.belowFloor) {
                  <z-badge zType="outline">below floor</z-badge>
                }
                <app-score-bar [score]="e.score" [scores]="scoresFor('entry', e.id)" />
              </div>
              @if (e.bullets.length) {
                <ul class="mt-2 ml-10 flex flex-col gap-1.5">
                  @for (b of e.bullets; track b.id) {
                    <li class="flex items-start gap-2 text-sm" [class.text-muted-foreground]="!b.included">
                      <input
                        type="checkbox"
                        class="accent-primary mt-0.5 size-3.5"
                        [checked]="b.included"
                        [disabled]="b.locked"
                        [title]="b.locked ? 'Every included entry keeps at least 1 bullet' : ''"
                        [attr.aria-label]="'Include bullet: ' + b.text"
                        (change)="ws.toggleBullet(b.id, $any($event.target).checked)"
                      />
                      <span class="flex-1">
                        {{ b.text }}
                        @if (b.dropped) {
                          <span class="text-xs text-amber-600">(dropped to fit)</span>
                        }
                      </span>
                      <app-score-bar [score]="b.score" [scores]="scoresFor('bullet', b.id)" />
                    </li>
                  }
                </ul>
              }
            </li>
          }
        </ul>
      </section>
    } @empty {
      <p class="text-muted-foreground p-6 text-center text-sm">Nothing ranked yet.</p>
    }
  `,
})
export class SelectionListComponent {
  protected readonly ws = inject(WorkspaceStore);
  private readonly byTarget = computed(() => {
    const m = new Map<string, ScoreRow[]>();
    for (const r of this.ws.scores()) {
      const k = `${r.target_type}:${r.target_id}`;
      m.set(k, [...(m.get(k) ?? []), r]);
    }
    return m;
  });
  protected readonly live = computed(() => {
    const c = this.ws.counts();
    return c ? `${c.entries} of ${c.totalEntries} entries and ${c.bullets} bullets selected` : '';
  });

  scoresFor(type: 'entry' | 'bullet', id: number): ScoreRow[] {
    return this.byTarget().get(`${type}:${id}`) ?? [];
  }

  drop(e: CdkDragDrop<unknown>, entries: EntryView[]): void {
    // Selected entries come first in each section, so the drop index is the
    // index among them; `moveEntry` clamps drops below the last one.
    this.move(entries[e.previousIndex], e.currentIndex);
  }

  key(ev: KeyboardEvent, e: EntryView): void {
    if (!ev.altKey || (ev.key !== 'ArrowUp' && ev.key !== 'ArrowDown')) return;
    ev.preventDefault();
    const group = this.ws.view().find((g) => g.entries.includes(e));
    const idx = group?.entries.filter((x) => x.included).findIndex((x) => x.id === e.id) ?? -1;
    if (idx >= 0) this.move(e, idx + (ev.key === 'ArrowUp' ? -1 : 1));
  }

  private move(e: EntryView, toIndex: number): void {
    const ids = moveEntry(this.ws.view(), e.id, toIndex);
    if (ids) this.ws.reorder(ids);
  }
}
