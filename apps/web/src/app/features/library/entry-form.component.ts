import { ChangeDetectionStrategy, Component, computed, inject, model } from '@angular/core';
import { FormsModule } from '@angular/forms';

import { ZardInputComponent } from '@/shared/components/input/input.component';
import { ZardSwitchComponent } from '@/shared/components/switch/switch.component';
import { KIND_LABELS } from '../../core/format';
import { ENTRY_KINDS, type EntryInput } from '../../core/ipc/types';
import { LibraryStore } from '../../core/stores/library.store';
import { BulletListEditorComponent } from './bullet-list-editor.component';
import { MonthFieldComponent } from './month-field.component';
import { SkillInputComponent } from './skill-input.component';

/** Problems that block saving; empty when the draft is valid. */
export function entryErrors(e: EntryInput): string[] {
  const errors: string[] = [];
  if (!e.title.trim()) errors.push('Title is required.');
  if (!/^\d{4}-\d{2}$/.test(e.start_date)) errors.push('Start date is required.');
  if (e.end_date && e.end_date < e.start_date) errors.push('End date is before the start date.');
  return errors;
}

/** Trims text and drops empty bullets before saving. */
export function cleanInput(e: EntryInput): EntryInput {
  return {
    ...e,
    title: e.title.trim(),
    org: e.org.trim(),
    location: e.location.trim(),
    description: e.description.trim(),
    bullets: e.bullets.map((b) => b.trim()).filter(Boolean),
  };
}

@Component({
  selector: 'app-entry-form',
  imports: [FormsModule, ZardInputComponent, ZardSwitchComponent, BulletListEditorComponent, SkillInputComponent, MonthFieldComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid gap-4">
      <div class="grid gap-1.5">
        <label class="text-sm font-medium" for="e-kind">Kind</label>
        <select
          id="e-kind"
          class="border-input h-8 rounded-md border bg-transparent px-2 text-sm"
          [ngModel]="draft().kind"
          (ngModelChange)="patch({ kind: $event })"
        >
          @for (k of kinds; track k) {
            <option [value]="k">{{ labels[k].one }}</option>
          }
        </select>
      </div>
      <div class="grid gap-1.5">
        <label class="text-sm font-medium" for="e-title">Title</label>
        <input z-input id="e-title" [ngModel]="draft().title" (ngModelChange)="patch({ title: $event })" placeholder="Senior Backend Engineer" />
      </div>
      <div class="grid grid-cols-2 gap-3">
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="e-org">{{ draft().kind === 'education' ? 'School' : 'Organization' }}</label>
          <input z-input id="e-org" [ngModel]="draft().org" (ngModelChange)="patch({ org: $event })" />
        </div>
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="e-loc">Location</label>
          <input z-input id="e-loc" [ngModel]="draft().location" (ngModelChange)="patch({ location: $event })" placeholder="Remote" />
        </div>
      </div>
      <div class="grid grid-cols-2 gap-3">
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="e-start">Start</label>
          <app-month-field fieldId="e-start" [value]="draft().start_date || null" (valueChange)="patch({ start_date: $event ?? '' })" />
        </div>
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="e-end">End</label>
          <app-month-field
            fieldId="e-end"
            [value]="draft().end_date"
            (valueChange)="patch({ end_date: $event })"
            [disabled]="current()"
            [placeholder]="current() ? 'Present' : 'Pick a month'"
          />
          <label class="text-muted-foreground flex items-center gap-2 text-xs">
            <input type="checkbox" [checked]="current()" (change)="setCurrent($any($event.target).checked)" /> Present
          </label>
        </div>
      </div>
      <div class="grid gap-1.5">
        <label class="text-sm font-medium" for="e-desc">Description</label>
        <textarea
          id="e-desc"
          rows="2"
          class="border-input w-full rounded-md border bg-transparent px-2.5 py-1.5 text-sm"
          [ngModel]="draft().description"
          (ngModelChange)="patch({ description: $event })"
          placeholder="One line of context (optional, not printed)"
        ></textarea>
      </div>
      <div class="grid gap-1.5">
        <span class="text-sm font-medium">Bullets</span>
        <app-bullet-list-editor [bullets]="draft().bullets" (bulletsChange)="patch({ bullets: $event })" />
      </div>
      <div class="grid gap-1.5">
        <span class="text-sm font-medium">Skills</span>
        <app-skill-input [skills]="library.skills()" [value]="draft().skills" (valueChange)="patch({ skills: $event })" />
      </div>
      @if (draft().id !== null) {
        <label class="flex items-center gap-2 text-sm">
          <z-switch [zChecked]="draft().archived" (zCheckedChange)="patch({ archived: $event })" /> Archived (hidden from matching)
        </label>
      }
    </div>
  `,
})
export class EntryFormComponent {
  readonly draft = model.required<EntryInput>();
  protected readonly library = inject(LibraryStore);
  protected readonly kinds = ENTRY_KINDS;
  protected readonly labels = KIND_LABELS;
  protected readonly current = computed(() => this.draft().end_date === null);

  patch(p: Partial<EntryInput>): void {
    this.draft.update((d) => ({ ...d, ...p }));
  }

  setCurrent(on: boolean): void {
    this.patch({ end_date: on ? null : this.draft().start_date || null });
  }
}
