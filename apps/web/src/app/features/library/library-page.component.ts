import { CdkDrag, CdkDragDrop, CdkDragHandle, CdkDropList } from '@angular/cdk/drag-drop';
import { ChangeDetectionStrategy, Component, computed, inject, input, signal, viewChild } from '@angular/core';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucideGripVertical, lucidePlus } from '@ng-icons/lucide';

import { ZardBadgeComponent } from '@/shared/components/badge/badge.component';
import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardCardImports } from '@/shared/components/card/card.imports';
import { ZardSkeletonComponent } from '@/shared/components/skeleton/skeleton.component';
import { formatRange, isEntryKind, KIND_LABELS } from '../../core/format';
import type { Entry, EntryKind } from '../../core/ipc/types';
import { emptyInput, LibraryStore, toInput } from '../../core/stores/library.store';
import { PageHeaderComponent } from '../../shell/page-header.component';
import { EntryEditorComponent } from './entry-editor.component';

/** Entries with a library order (`sort_hint` > 0) come first, ascending; the rest follow by date. */
const hintRank = (e: Entry) => (e.sort_hint > 0 ? e.sort_hint : Number.MAX_SAFE_INTEGER);

@Component({
  selector: 'app-library-page',
  imports: [
    PageHeaderComponent,
    EntryEditorComponent,
    ZardButtonComponent,
    ZardBadgeComponent,
    ZardSkeletonComponent,
    ...ZardCardImports,
    NgIcon,
    CdkDropList,
    CdkDrag,
    CdkDragHandle,
  ],
  viewProviders: [provideIcons({ lucideGripVertical, lucidePlus })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'Library', link: '/library/experience' }, { label: label().many }]">
      <label class="text-muted-foreground flex items-center gap-2 text-xs">
        <input type="checkbox" [checked]="showArchived()" (change)="showArchived.set($any($event.target).checked)" /> Show archived
      </label>
      <button z-button zSize="sm" (click)="create()"><ng-icon name="lucidePlus" /> New</button>
    </app-page-header>

    <div class="flex flex-1 flex-col gap-4 p-4">
      <div class="grid auto-rows-min gap-4 sm:grid-cols-2 lg:grid-cols-4">
        @for (c of cards(); track c.label) {
          <z-card class="gap-1 py-4">
            <z-card-content>
              <div class="text-muted-foreground text-xs">{{ c.label }}</div>
              <div class="text-2xl font-semibold tabular-nums">{{ c.value }}</div>
            </z-card-content>
          </z-card>
        }
      </div>

      @if (!library.loaded()) {
        <div class="grid gap-2">
          @for (i of [1, 2, 3]; track i) {
            <z-skeleton class="h-16 w-full" />
          }
        </div>
      } @else if (visible().length === 0) {
        <div class="text-muted-foreground flex flex-1 flex-col items-center justify-center gap-3 rounded-xl border border-dashed p-10 text-center">
          <p class="text-foreground font-medium">No {{ label().many.toLowerCase() }} yet</p>
          <p class="max-w-sm text-sm">{{ hint() }}</p>
          <button z-button (click)="create()"><ng-icon name="lucidePlus" /> New {{ label().one.toLowerCase() }}</button>
        </div>
      } @else {
        <p class="text-muted-foreground text-xs">Drag to set the default order of these entries on a CV.</p>
        <ul cdkDropList (cdkDropListDropped)="drop($event)" class="divide-y rounded-xl border" aria-label="Entries">
          @for (e of visible(); track e.id) {
            <li cdkDrag class="bg-background flex items-center">
              <button
                type="button"
                cdkDragHandle
                class="text-muted-foreground hover:text-foreground cursor-grab px-3"
                [attr.aria-label]="'Reorder ' + e.title + ' (Alt+Up/Down)'"
                (keydown)="key($event, e)"
              >
                <ng-icon name="lucideGripVertical" class="size-4" />
              </button>
              <button
                type="button"
                class="hover:bg-muted/50 focus-visible:bg-muted/50 flex w-full flex-col gap-1 py-3 pr-4 text-left outline-none"
                (click)="open(e)"
              >
                <div class="flex items-baseline justify-between gap-3">
                  <span class="font-medium">
                    {{ e.title }}
                    @if (e.org) {
                      <span class="text-muted-foreground font-normal">· {{ e.org }}</span>
                    }
                  </span>
                  <span class="text-muted-foreground shrink-0 text-xs tabular-nums">{{ range(e) }}</span>
                </div>
                <div class="flex flex-wrap items-center gap-1.5">
                  <span class="text-muted-foreground text-xs">{{ e.bullets.length }} bullet{{ e.bullets.length === 1 ? '' : 's' }}</span>
                  @if (e.archived) {
                    <z-badge zType="outline">archived</z-badge>
                  }
                  @for (s of e.skills; track s.id) {
                    <z-badge zType="secondary">{{ s.name }}</z-badge>
                  }
                </div>
              </button>
            </li>
          }
        </ul>
      }
    </div>

    <app-entry-editor />
  `,
})
export class LibraryPageComponent {
  /** Route param. */
  readonly kind = input<string>('experience');
  protected readonly library = inject(LibraryStore);
  private readonly editor = viewChild.required(EntryEditorComponent);
  protected readonly showArchived = signal(false);

  protected readonly safeKind = computed<EntryKind>(() => (isEntryKind(this.kind()) ? (this.kind() as EntryKind) : 'experience'));
  protected readonly label = computed(() => KIND_LABELS[this.safeKind()]);
  protected readonly all = computed(() => this.library.byKind(this.safeKind(), true));
  protected readonly visible = computed(() =>
    this.all()
      .filter((e) => this.showArchived() || !e.archived)
      .sort((a, b) => hintRank(a) - hintRank(b) || (b.end_date ?? '9999').localeCompare(a.end_date ?? '9999') || b.start_date.localeCompare(a.start_date)),
  );
  protected readonly cards = computed(() => {
    const all = this.all();
    const active = all.filter((e) => !e.archived);
    return [
      { label: this.label().many, value: active.length },
      { label: 'Bullets', value: active.reduce((n, e) => n + e.bullets.length, 0) },
      { label: 'Linked skills', value: new Set(active.flatMap((e) => e.skills.map((s) => s.id))).size },
      { label: 'Archived', value: all.length - active.length },
    ];
  });
  protected readonly hint = computed(
    () =>
      ({
        experience: 'Add your jobs. Each bullet is ranked on its own against a job description, so write one achievement per bullet.',
        project: 'Side projects, open source, or notable work that is not a job.',
        education: 'Degrees, bootcamps and certifications.',
        volunteer: 'Volunteer roles and community work.',
      })[this.safeKind()],
  );
  protected readonly range = (e: Entry) => formatRange(e.start_date, e.end_date);

  drop(ev: CdkDragDrop<unknown>): void {
    const list = [...this.visible()];
    this.reposition(list, ev.previousIndex, ev.currentIndex);
  }

  key(ev: KeyboardEvent, e: Entry): void {
    if (!ev.altKey || (ev.key !== 'ArrowUp' && ev.key !== 'ArrowDown')) return;
    ev.preventDefault();
    const list = [...this.visible()];
    const i = list.findIndex((x) => x.id === e.id);
    this.reposition(list, i, i + (ev.key === 'ArrowUp' ? -1 : 1));
  }

  private reposition(list: Entry[], from: number, to: number): void {
    if (from < 0 || to < 0 || to >= list.length || from === to) return;
    list.splice(to, 0, ...list.splice(from, 1));
    void this.library.setOrder(list);
  }

  create(): void {
    this.editor().edit(emptyInput(this.safeKind()));
  }

  open(e: Entry): void {
    this.editor().edit(toInput(e));
  }
}
