import { ChangeDetectionStrategy, Component, computed, inject, input, model, output, signal } from '@angular/core';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSheetImports } from '@/shared/components/sheet/sheet.imports';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { KIND_LABELS } from '../../core/format';
import { messageFor } from '../../core/ipc/errors';
import type { EntryInput } from '../../core/ipc/types';
import { LibraryStore } from '../../core/stores/library.store';
import { cleanInput, EntryFormComponent, entryErrors } from './entry-form.component';

/** Side-sheet editor for one library entry. */
@Component({
  selector: 'app-entry-editor',
  imports: [...ZardSheetImports, ZardButtonComponent, EntryFormComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <z-sheet [(zVisible)]="open" zSide="right" zWidth="min(560px, 100vw)" (zVisibleChange)="!$event && closed.emit()">
      <z-sheet-header>
        <z-sheet-title>{{ title() }}</z-sheet-title>
        <z-sheet-description>Your own words only. The app selects and orders them; it never rewrites.</z-sheet-description>
      </z-sheet-header>
      <div class="flex-1 overflow-y-auto px-4 pb-4">
        @if (draft(); as d) {
          <app-entry-form [draft]="d" (draftChange)="draft.set($event)" />
        }
        @if (touched() && errors().length) {
          <ul class="text-destructive mt-3 list-disc pl-5 text-sm" role="alert">
            @for (e of errors(); track e) {
              <li>{{ e }}</li>
            }
          </ul>
        }
      </div>
      <z-sheet-footer class="flex-row justify-between">
        <div>
          @if (entryId() !== null) {
            @if (confirmDelete()) {
              <button z-button zType="destructive" type="button" (click)="remove()">Confirm delete</button>
            } @else {
              <button z-button zType="ghost" type="button" (click)="confirmDelete.set(true)">Delete</button>
            }
          }
        </div>
        <div class="flex gap-2">
          <button z-button zType="outline" type="button" z-sheet-close>Cancel</button>
          <button z-button type="button" [zLoading]="saving()" (click)="save()">Save</button>
        </div>
      </z-sheet-footer>
    </z-sheet>
  `,
})
export class EntryEditorComponent {
  readonly open = model(false);
  /** The entry being edited; set before opening. */
  readonly initial = input<EntryInput | null>(null);
  readonly closed = output<void>();
  private readonly library = inject(LibraryStore);
  private readonly toast = inject(ZardSonnerService);

  protected readonly draft = signal<EntryInput | null>(null);
  protected readonly saving = signal(false);
  protected readonly touched = signal(false);
  protected readonly confirmDelete = signal(false);
  protected readonly entryId = computed(() => this.draft()?.id ?? null);
  protected readonly errors = computed(() => (this.draft() ? entryErrors(this.draft()!) : []));
  protected readonly title = computed(() => {
    const d = this.draft();
    if (!d) return '';
    return `${d.id === null ? 'New' : 'Edit'} ${KIND_LABELS[d.kind].one.toLowerCase()}`;
  });

  /** Opens the sheet on a copy of `entry`. */
  edit(entry: EntryInput): void {
    this.draft.set(structuredClone(entry));
    this.touched.set(false);
    this.confirmDelete.set(false);
    this.open.set(true);
  }

  async save(): Promise<void> {
    this.touched.set(true);
    const d = this.draft();
    if (!d || this.errors().length) return;
    this.saving.set(true);
    try {
      await this.library.save(cleanInput(d));
      this.toast.success('Saved');
      this.open.set(false);
    } catch (e) {
      this.toast.error('Could not save', { description: messageFor(e) });
    } finally {
      this.saving.set(false);
    }
  }

  async remove(): Promise<void> {
    const id = this.entryId();
    if (id === null) return;
    try {
      await this.library.remove(id);
      this.toast.success('Deleted');
      this.open.set(false);
    } catch (e) {
      this.toast.error('Could not delete', { description: messageFor(e) });
    }
  }
}
