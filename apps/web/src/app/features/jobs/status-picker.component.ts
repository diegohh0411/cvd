import { CdkDrag, type CdkDragDrop, CdkDragHandle, CdkDropList, moveItemInArray } from '@angular/cdk/drag-drop';
import { ChangeDetectionStrategy, Component, computed, inject, input, type OnInit, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucideCheck, lucideChevronLeft, lucideEllipsis, lucideGripVertical, lucidePlus, lucideTrash2, lucideX } from '@ng-icons/lucide';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardInputComponent } from '@/shared/components/input/input.component';
import { ZardPopoverImports } from '@/shared/components/popover/popover.imports';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { messageFor } from '../../core/ipc/errors';
import { STATUS_COLORS, type JobStatus, type StatusColor } from '../../core/ipc/types';
import { JobStatusesStore } from '../../core/stores/job-statuses.store';
import { StatusPillComponent, SWATCH_CLASSES } from './status-pill.component';

/**
 * Notion-style select: pick a status, type to filter or create one, drag to
 * reorder, and open "⋯" on a row to rename, recolor or delete it.
 */
@Component({
  selector: 'app-status-picker',
  imports: [
    FormsModule,
    NgIcon,
    CdkDropList,
    CdkDrag,
    CdkDragHandle,
    ZardButtonComponent,
    ZardInputComponent,
    ...ZardPopoverImports,
    StatusPillComponent,
  ],
  viewProviders: [
    provideIcons({ lucideCheck, lucideChevronLeft, lucideEllipsis, lucideGripVertical, lucidePlus, lucideTrash2, lucideX }),
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <button
      type="button"
      class="hover:bg-muted focus-visible:ring-ring/50 inline-flex h-7 max-w-full min-w-0 items-center rounded-md px-1 outline-none focus-visible:ring-[3px]"
      zPopover
      [zContent]="panel"
      zAlign="start"
      [zVisible]="open()"
      (zVisibleChange)="onVisible($event)"
      [attr.aria-label]="'Status: ' + (current()?.name ?? 'none')"
    >
      @if (current(); as s) {
        <app-status-pill [name]="s.name" [color]="s.color" />
      } @else {
        <span class="text-muted-foreground px-1 text-xs">{{ placeholder() }}</span>
      }
    </button>

    <ng-template #panel>
      <z-popover class="w-64 gap-0 p-0">
        @if (editing(); as e) {
          <div class="flex flex-col gap-1 p-2">
            <div class="flex items-center gap-1">
              <button z-button zType="ghost" zSize="icon-xs" type="button" aria-label="Back" (click)="closeEditor()">
                <ng-icon name="lucideChevronLeft" />
              </button>
              <input
                z-input
                class="h-7 text-sm"
                aria-label="Status name"
                [ngModel]="draftName()"
                (ngModelChange)="draftName.set($event)"
                (keydown.enter)="$event.preventDefault(); saveName(e)"
                (blur)="saveName(e)"
              />
            </div>
            @if (confirmDelete()) {
              <div class="flex items-center justify-between gap-2 px-1 py-1 text-xs">
                <span class="text-muted-foreground">Jobs using it will have no status.</span>
                <button z-button zType="destructive" zSize="xs" type="button" (click)="remove(e)">Delete</button>
              </div>
            } @else {
              <button
                type="button"
                class="hover:bg-muted text-destructive flex items-center gap-2 rounded-md px-2 py-1.5 text-sm"
                (click)="confirmDelete.set(true)"
              >
                <ng-icon name="lucideTrash2" /> Delete
              </button>
            }
            <div class="text-muted-foreground px-2 pt-2 pb-1 text-xs font-medium">Colors</div>
            <ul role="listbox" aria-label="Color">
              @for (c of colors; track c) {
                <li>
                  <button
                    type="button"
                    role="option"
                    [attr.aria-selected]="e.color === c"
                    class="hover:bg-muted flex w-full items-center gap-2 rounded-md px-2 py-1 text-sm capitalize"
                    (click)="recolor(e, c)"
                  >
                    <span class="size-4 rounded border border-black/10" [class]="swatch[c]"></span>
                    <span class="flex-1 text-left">{{ c }}</span>
                    @if (e.color === c) {
                      <ng-icon name="lucideCheck" class="size-4" />
                    }
                  </button>
                </li>
              }
            </ul>
          </div>
        } @else {
          <div class="border-b p-2">
            <input
              z-input
              class="h-7 text-sm"
              placeholder="Search or create…"
              aria-label="Search or create a status"
              [ngModel]="query()"
              (ngModelChange)="query.set($event)"
              (keydown.enter)="$event.preventDefault(); submitQuery()"
            />
          </div>
          <div class="text-muted-foreground px-3 pt-2 pb-1 text-xs">Select an option or create one</div>
          <ul
            role="listbox"
            aria-label="Statuses"
            class="max-h-64 overflow-y-auto px-1 pb-1"
            cdkDropList
            [cdkDropListDisabled]="!!query().trim()"
            (cdkDropListDropped)="drop($event)"
          >
            @for (s of filtered(); track s.id) {
              <li cdkDrag cdkDragLockAxis="y" class="group bg-popover flex items-center gap-1 rounded-md pr-1 hover:bg-muted">
                <span cdkDragHandle class="text-muted-foreground cursor-grab px-0.5 opacity-0 group-hover:opacity-100" aria-hidden="true">
                  <ng-icon name="lucideGripVertical" class="size-3.5" />
                </span>
                <button
                  type="button"
                  role="option"
                  [attr.aria-selected]="s.id === value()"
                  class="flex min-w-0 flex-1 items-center gap-2 py-1 text-left"
                  (click)="select(s.id)"
                >
                  <app-status-pill [name]="s.name" [color]="s.color" />
                  @if (s.id === value()) {
                    <ng-icon name="lucideCheck" class="ml-auto size-4" />
                  }
                </button>
                <button
                  z-button
                  zType="ghost"
                  zSize="icon-xs"
                  type="button"
                  class="opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                  [attr.aria-label]="'Edit ' + s.name"
                  (click)="openEditor(s)"
                >
                  <ng-icon name="lucideEllipsis" />
                </button>
              </li>
            }
            @if (canCreate()) {
              <li>
                <button type="button" class="hover:bg-muted flex w-full items-center gap-2 rounded-md px-2 py-1 text-sm" (click)="create()">
                  <ng-icon name="lucidePlus" class="text-muted-foreground size-3.5" />
                  Create <app-status-pill [name]="query().trim()" [color]="store.nextColor()" />
                </button>
              </li>
            }
          </ul>
          @if (value() !== null) {
            <div class="border-t p-1">
              <button
                type="button"
                class="hover:bg-muted text-muted-foreground flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-sm"
                (click)="select(null)"
              >
                <ng-icon name="lucideX" class="size-3.5" /> Clear status
              </button>
            </div>
          }
        }
      </z-popover>
    </ng-template>
  `,
})
export class StatusPickerComponent implements OnInit {
  protected readonly store = inject(JobStatusesStore);
  private readonly toast = inject(ZardSonnerService);

  readonly value = input<number | null>(null);
  readonly placeholder = input('Set status');
  readonly valueChange = output<number | null>();

  protected readonly colors = STATUS_COLORS;
  protected readonly swatch = SWATCH_CLASSES;
  protected readonly open = signal(false);
  protected readonly query = signal('');
  protected readonly editingId = signal<number | null>(null);
  protected readonly draftName = signal('');
  protected readonly confirmDelete = signal(false);

  protected readonly current = computed(() => {
    const v = this.value();
    return v === null ? undefined : this.store.byId().get(v);
  });
  protected readonly editing = computed(() => {
    const id = this.editingId();
    return id === null ? undefined : this.store.byId().get(id);
  });
  protected readonly filtered = computed(() => {
    const q = this.query().trim().toLowerCase();
    return this.store.statuses().filter((s) => s.name.toLowerCase().includes(q));
  });
  protected readonly canCreate = computed(() => {
    const q = this.query().trim().toLowerCase();
    return !!q && !this.store.statuses().some((s) => s.name.toLowerCase() === q);
  });

  ngOnInit(): void {
    void this.store.ensureLoaded();
  }

  protected onVisible(v: boolean): void {
    this.open.set(v);
    if (!v) {
      this.query.set('');
      this.editingId.set(null);
    }
  }

  protected select(id: number | null): void {
    if (id !== this.value()) this.valueChange.emit(id);
    this.onVisible(false);
  }

  protected async submitQuery(): Promise<void> {
    const q = this.query().trim().toLowerCase();
    const exact = this.store.statuses().find((s) => s.name.toLowerCase() === q);
    if (exact) this.select(exact.id);
    else if (q) await this.create();
  }

  protected async create(): Promise<void> {
    await this.attempt(async () => {
      const s = await this.store.create(this.query().trim());
      this.select(s.id);
    });
  }

  protected openEditor(s: JobStatus): void {
    this.draftName.set(s.name);
    this.confirmDelete.set(false);
    this.editingId.set(s.id);
  }

  protected closeEditor(): void {
    const e = this.editing();
    if (e) void this.saveName(e);
    this.editingId.set(null);
  }

  protected async saveName(s: JobStatus): Promise<void> {
    const name = this.draftName().trim();
    if (!name || name === s.name) return this.draftName.set(s.name);
    await this.attempt(() => this.store.update(s.id, name, s.color), () => this.draftName.set(s.name));
  }

  protected async recolor(s: JobStatus, color: StatusColor): Promise<void> {
    await this.attempt(() => this.store.update(s.id, s.name, color));
  }

  protected async remove(s: JobStatus): Promise<void> {
    await this.attempt(async () => {
      await this.store.remove(s.id);
      this.editingId.set(null);
      if (this.value() === s.id) this.valueChange.emit(null);
    });
  }

  protected async drop(e: CdkDragDrop<unknown>): Promise<void> {
    const ids = this.store.statuses().map((s) => s.id);
    moveItemInArray(ids, e.previousIndex, e.currentIndex);
    await this.attempt(() => this.store.reorder(ids));
  }

  private async attempt(fn: () => Promise<void>, onError?: () => void): Promise<void> {
    try {
      await fn();
    } catch (err) {
      onError?.();
      this.toast.error('Could not update status', { description: messageFor(err) });
    }
  }
}
