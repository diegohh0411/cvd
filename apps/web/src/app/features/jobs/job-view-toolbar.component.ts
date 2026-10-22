import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { NgIcon, provideIcons } from '@ng-icons/core';
import {
  lucideArrowDownUp,
  lucideCopy,
  lucideKanban,
  lucideListFilter,
  lucidePlus,
  lucideSheet,
  lucideTrash2,
  lucideX,
} from '@ng-icons/lucide';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardInputComponent } from '@/shared/components/input/input.component';
import { ZardPopoverImports } from '@/shared/components/popover/popover.imports';
import { ZardSelectImports } from '@/shared/components/select/select.imports';
import { JobStatusesStore } from '../../core/stores/job-statuses.store';
import {
  type FieldKey,
  fieldLabel,
  FIELDS,
  type FilterOp,
  type JobFilter,
  type JobSort,
  type JobView,
  JobViewsStore,
  newFilter,
  OPS_BY_FIELD,
  type ViewLayout,
} from './job-views';
import { StatusPillComponent } from './status-pill.component';

/** Notion-style view tabs plus the active view's filter and sort editors. */
@Component({
  selector: 'app-job-view-toolbar',
  imports: [FormsModule, NgIcon, ZardButtonComponent, ZardInputComponent, ...ZardPopoverImports, ...ZardSelectImports, StatusPillComponent],
  viewProviders: [
    provideIcons({ lucideArrowDownUp, lucideCopy, lucideKanban, lucideListFilter, lucidePlus, lucideSheet, lucideTrash2, lucideX }),
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'flex flex-col gap-2' },
  template: `
    <div class="flex items-center gap-2 border-b">
      <div class="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto" role="tablist" aria-label="Views">
        @for (v of store.views(); track v.id) {
          @let active = v.id === view().id;
          <button
            type="button"
            role="tab"
            [attr.aria-selected]="active"
            class="-mb-px flex h-8 shrink-0 items-center gap-1.5 border-b-2 px-2 text-sm"
            [class]="active ? 'border-foreground text-foreground font-medium' : 'text-muted-foreground hover:text-foreground border-transparent'"
            zPopover
            [zTrigger]="null"
            [zContent]="viewMenu"
            zAlign="start"
            [zVisible]="menuFor() === v.id"
            (zVisibleChange)="!$event && closeMenu()"
            (click)="active ? openMenu(v) : store.activeId.set(v.id)"
          >
            <ng-icon [name]="v.layout === 'board' ? 'lucideKanban' : 'lucideSheet'" class="size-3.5" />
            {{ v.name }}
          </button>
        }
        <button
          z-button
          zType="ghost"
          zSize="icon-xs"
          type="button"
          aria-label="Add view"
          zPopover
          [zContent]="addMenu"
          zAlign="start"
          [zVisible]="adding()"
          (zVisibleChange)="adding.set($event)"
        >
          <ng-icon name="lucidePlus" />
        </button>
      </div>

      <button
        z-button
        zType="ghost"
        zSize="xs"
        type="button"
        data-testid="filter-button"
        zPopover
        [zContent]="filterPanel"
        zAlign="end"
        [class.text-primary]="view().filters.length"
      >
        <ng-icon name="lucideListFilter" /> Filter
      </button>
      <button
        z-button
        zType="ghost"
        zSize="xs"
        type="button"
        data-testid="sort-button"
        zPopover
        [zContent]="sortPanel"
        zAlign="end"
        [class.text-primary]="view().sorts.length"
      >
        <ng-icon name="lucideArrowDownUp" /> Sort
      </button>
    </div>

    @if (view().filters.length || view().sorts.length) {
      <div class="flex flex-wrap items-center gap-1.5 text-xs">
        @for (s of view().sorts; track $index) {
          <span class="bg-muted inline-flex items-center gap-1 rounded-full px-2 py-0.5">
            <ng-icon name="lucideArrowDownUp" class="size-3" /> {{ label(s.field) }} {{ s.dir === 'asc' ? '↑' : '↓' }}
          </span>
        }
        @for (f of view().filters; track $index) {
          <span class="bg-muted inline-flex items-center gap-1 rounded-full px-2 py-0.5">
            {{ summary(f) }}
            <button type="button" class="hover:text-foreground text-muted-foreground" [attr.aria-label]="'Remove filter'" (click)="removeFilter($index)">
              <ng-icon name="lucideX" class="size-3" />
            </button>
          </span>
        }
      </div>
    }

    <ng-template #viewMenu>
      <z-popover class="w-60 gap-1 p-2">
        <input
          z-input
          class="h-7 text-sm"
          aria-label="View name"
          [ngModel]="draftName()"
          (ngModelChange)="draftName.set($event)"
          (keydown.enter)="$event.preventDefault(); closeMenu()"
        />
        <div class="text-muted-foreground px-1 pt-2 pb-1 text-xs font-medium">Layout</div>
        @for (l of layouts; track l.key) {
          <button type="button" class="hover:bg-muted flex items-center gap-2 rounded-md px-2 py-1.5 text-sm" [class.bg-muted]="view().layout === l.key" (click)="setLayout(l.key)">
            <ng-icon [name]="l.icon" class="size-3.5" /> {{ l.label }}
          </button>
        }
        <div class="my-1 border-t"></div>
        <button type="button" class="hover:bg-muted flex items-center gap-2 rounded-md px-2 py-1.5 text-sm" (click)="duplicate()">
          <ng-icon name="lucideCopy" class="size-3.5" /> Duplicate view
        </button>
        <button
          type="button"
          class="hover:bg-muted text-destructive flex items-center gap-2 rounded-md px-2 py-1.5 text-sm disabled:opacity-50"
          [disabled]="store.views().length <= 1"
          (click)="remove()"
        >
          <ng-icon name="lucideTrash2" class="size-3.5" /> Delete view
        </button>
      </z-popover>
    </ng-template>

    <ng-template #addMenu>
      <z-popover class="w-48 gap-0 p-1">
        @for (l of layouts; track l.key) {
          <button type="button" class="hover:bg-muted flex items-center gap-2 rounded-md px-2 py-1.5 text-sm" (click)="add(l.key)">
            <ng-icon [name]="l.icon" class="size-3.5" /> {{ l.label }}
          </button>
        }
      </z-popover>
    </ng-template>

    <ng-template #filterPanel>
      <z-popover class="w-[30rem] gap-2 p-2">
        @for (f of view().filters; track $index; let i = $index) {
          <div class="flex items-center gap-1.5">
            <z-select class="w-28" [zValue]="f.field" (zValueChange)="setFilterField(i, $any($event))">
              @for (fd of fields; track fd.key) {
                <z-select-item [zValue]="fd.key">{{ fd.label }}</z-select-item>
              }
            </z-select>
            <z-select class="w-36" [zValue]="f.op" (zValueChange)="patchFilter(i, { op: $any($event) })">
              @for (o of ops[f.field]; track o.op) {
                <z-select-item [zValue]="o.op">{{ o.label }}</z-select-item>
              }
            </z-select>
            @if (f.op !== 'empty' && f.op !== 'not_empty') {
              @if (f.field === 'status') {
                <div class="flex min-w-0 flex-1 flex-wrap gap-1">
                  @for (s of statuses.statuses(); track s.id) {
                    <button
                      type="button"
                      class="rounded ring-offset-1"
                      [class.ring-2]="has(f, s.id)"
                      [class.ring-ring]="has(f, s.id)"
                      [class.opacity-50]="!has(f, s.id)"
                      [attr.aria-pressed]="has(f, s.id)"
                      (click)="toggleStatus(i, s.id)"
                    >
                      <app-status-pill [name]="s.name" [color]="s.color" />
                    </button>
                  }
                </div>
              } @else {
                <input
                  z-input
                  class="h-8 flex-1 text-sm"
                  [type]="f.field === 'created' ? 'date' : 'text'"
                  aria-label="Filter value"
                  placeholder="Value…"
                  [ngModel]="f.value"
                  (ngModelChange)="patchFilter(i, { value: $event })"
                />
              }
            } @else {
              <span class="flex-1"></span>
            }
            <button z-button zType="ghost" zSize="icon-xs" type="button" aria-label="Remove filter" (click)="removeFilter(i)">
              <ng-icon name="lucideX" />
            </button>
          </div>
        } @empty {
          <div class="text-muted-foreground px-1 text-xs">No filters in this view.</div>
        }
        <div class="flex flex-wrap gap-1 border-t pt-2">
          @for (fd of fields; track fd.key) {
            <button z-button zType="ghost" zSize="xs" type="button" (click)="addFilter(fd.key)"><ng-icon name="lucidePlus" /> {{ fd.label }}</button>
          }
        </div>
      </z-popover>
    </ng-template>

    <ng-template #sortPanel>
      <z-popover class="w-80 gap-2 p-2">
        @for (s of view().sorts; track $index; let i = $index) {
          <div class="flex items-center gap-1.5">
            <z-select class="flex-1" [zValue]="s.field" (zValueChange)="patchSort(i, { field: $any($event) })">
              @for (fd of fields; track fd.key) {
                <z-select-item [zValue]="fd.key">{{ fd.label }}</z-select-item>
              }
            </z-select>
            <z-select class="w-32" [zValue]="s.dir" (zValueChange)="patchSort(i, { dir: $any($event) })">
              <z-select-item zValue="asc">Ascending</z-select-item>
              <z-select-item zValue="desc">Descending</z-select-item>
            </z-select>
            <button z-button zType="ghost" zSize="icon-xs" type="button" aria-label="Remove sort" (click)="removeSort(i)">
              <ng-icon name="lucideX" />
            </button>
          </div>
        } @empty {
          <div class="text-muted-foreground px-1 text-xs">Newest first by default.</div>
        }
        @if (unsorted().length) {
          <div class="flex flex-wrap gap-1 border-t pt-2">
            @for (fd of unsorted(); track fd.key) {
              <button z-button zType="ghost" zSize="xs" type="button" (click)="addSort(fd.key)"><ng-icon name="lucidePlus" /> {{ fd.label }}</button>
            }
          </div>
        }
      </z-popover>
    </ng-template>
  `,
})
export class JobViewToolbarComponent {
  protected readonly store = inject(JobViewsStore);
  protected readonly statuses = inject(JobStatusesStore);

  protected readonly fields = FIELDS;
  protected readonly ops = OPS_BY_FIELD;
  protected readonly label = fieldLabel;
  protected readonly layouts: { key: ViewLayout; label: string; icon: string }[] = [
    { key: 'table', label: 'Table', icon: 'lucideSheet' },
    { key: 'board', label: 'Board', icon: 'lucideKanban' },
  ];

  protected readonly view = computed(() => this.store.active()!);
  protected readonly unsorted = computed(() => FIELDS.filter((f) => !this.view().sorts.some((s) => s.field === f.key)));
  protected readonly menuFor = signal<number | null>(null);
  protected readonly adding = signal(false);
  protected readonly draftName = signal('');

  protected openMenu(v: JobView): void {
    this.draftName.set(v.name);
    this.menuFor.set(v.id);
  }

  protected closeMenu(): void {
    const id = this.menuFor();
    const name = this.draftName().trim();
    if (id !== null && name && name !== this.store.views().find((v) => v.id === id)?.name) this.store.patch(id, { name });
    this.menuFor.set(null);
  }

  protected setLayout(layout: ViewLayout): void {
    this.store.patch(this.view().id, { layout });
  }

  protected duplicate(): void {
    const id = this.view().id;
    this.closeMenu();
    void this.store.duplicate(id);
  }

  protected remove(): void {
    const id = this.view().id;
    this.menuFor.set(null);
    void this.store.remove(id);
  }

  protected add(layout: ViewLayout): void {
    this.adding.set(false);
    void this.store.add(layout);
  }

  protected summary(f: JobFilter): string {
    const op = OPS_BY_FIELD[f.field].find((o) => o.op === f.op)?.label ?? f.op;
    if (f.op === 'empty' || f.op === 'not_empty') return `${fieldLabel(f.field)} ${op}`;
    const value = Array.isArray(f.value)
      ? f.value.map((id) => this.statuses.byId().get(id)?.name ?? '?').join(', ') || '…'
      : f.value || '…';
    return `${fieldLabel(f.field)} ${op} ${value}`;
  }

  protected has(f: JobFilter, id: number): boolean {
    return Array.isArray(f.value) && f.value.includes(id);
  }

  private setFilters(fn: (fs: JobFilter[]) => JobFilter[]): void {
    this.store.patch(this.view().id, { filters: fn(this.view().filters) });
  }

  private setSorts(fn: (ss: JobSort[]) => JobSort[]): void {
    this.store.patch(this.view().id, { sorts: fn(this.view().sorts) });
  }

  protected addFilter(field: FieldKey): void {
    this.setFilters((fs) => [...fs, newFilter(field)]);
  }

  protected setFilterField(i: number, field: FieldKey): void {
    if (this.view().filters[i]?.field !== field) this.setFilters((fs) => fs.map((f, j) => (j === i ? newFilter(field) : f)));
  }

  protected patchFilter(i: number, change: { op?: FilterOp; value?: string }): void {
    this.setFilters((fs) => fs.map((f, j) => (j === i ? { ...f, ...change } : f)));
  }

  protected toggleStatus(i: number, id: number): void {
    this.setFilters((fs) =>
      fs.map((f, j) => {
        if (j !== i) return f;
        const ids = f.value as number[];
        return { ...f, value: ids.includes(id) ? ids.filter((x) => x !== id) : [...ids, id] };
      }),
    );
  }

  protected removeFilter(i: number): void {
    this.setFilters((fs) => fs.filter((_, j) => j !== i));
  }

  protected addSort(field: FieldKey): void {
    this.setSorts((ss) => [...ss, { field, dir: 'asc' }]);
  }

  protected patchSort(i: number, change: Partial<JobSort>): void {
    this.setSorts((ss) => ss.map((s, j) => (j === i ? { ...s, ...change } : s)));
  }

  protected removeSort(i: number): void {
    this.setSorts((ss) => ss.filter((_, j) => j !== i));
  }
}
