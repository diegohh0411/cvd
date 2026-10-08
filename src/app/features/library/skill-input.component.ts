import { ChangeDetectionStrategy, Component, computed, input, model, signal } from '@angular/core';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucideX } from '@ng-icons/lucide';

import { ZardBadgeComponent } from '@/shared/components/badge/badge.component';
import type { Skill } from '../../core/ipc/types';

export interface SkillOption {
  label: string;
  create: boolean;
}

/** Matches existing skills case-insensitively and offers to create an unknown name. */
export function skillOptions(query: string, all: readonly Skill[], chosen: readonly string[], limit = 6): SkillOption[] {
  const q = query.trim();
  const taken = new Set(chosen.map((c) => c.toLowerCase()));
  const matches = all
    .filter((s) => !taken.has(s.name.toLowerCase()) && s.name.toLowerCase().includes(q.toLowerCase()))
    .slice(0, limit)
    .map((s) => ({ label: s.name, create: false }));
  const exact = all.some((s) => s.name.toLowerCase() === q.toLowerCase()) || taken.has(q.toLowerCase());
  return q && !exact ? [...matches, { label: q, create: true }] : q ? matches : [];
}

@Component({
  selector: 'app-skill-input',
  imports: [ZardBadgeComponent, NgIcon],
  viewProviders: [provideIcons({ lucideX })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="border-input focus-within:ring-ring/50 focus-within:border-ring flex flex-wrap items-center gap-1.5 rounded-md border px-2 py-1.5 focus-within:ring-[3px]">
      @for (s of value(); track s) {
        <z-badge zType="secondary" class="gap-1">
          {{ s }}
          <button type="button" (click)="remove(s)" [attr.aria-label]="'Remove ' + s" class="hover:text-foreground">
            <ng-icon name="lucideX" class="size-3" />
          </button>
        </z-badge>
      }
      <input
        class="min-w-24 flex-1 bg-transparent text-sm outline-none"
        placeholder="Add a skill…"
        role="combobox"
        aria-autocomplete="list"
        [attr.aria-expanded]="options().length > 0"
        aria-controls="skill-options"
        [value]="query()"
        (input)="query.set($any($event.target).value); active.set(0)"
        (keydown)="key($event)"
      />
    </div>
    @if (options().length) {
      <ul id="skill-options" role="listbox" class="bg-popover mt-1 rounded-md border p-1 text-sm shadow-md">
        @for (o of options(); track o.label; let i = $index) {
          <li
            role="option"
            [attr.aria-selected]="i === active()"
            class="cursor-pointer rounded px-2 py-1"
            [class.bg-accent]="i === active()"
            (mousedown)="$event.preventDefault(); pick(o)"
          >
            {{ o.create ? 'Create “' + o.label + '”' : o.label }}
          </li>
        }
      </ul>
    }
  `,
})
export class SkillInputComponent {
  readonly value = model<string[]>([]);
  readonly skills = input<Skill[]>([]);
  protected readonly query = signal('');
  protected readonly active = signal(0);
  protected readonly options = computed(() => skillOptions(this.query(), this.skills(), this.value()));

  pick(o: SkillOption): void {
    this.value.update((v) => [...v, o.label]);
    this.query.set('');
  }

  remove(s: string): void {
    this.value.update((v) => v.filter((x) => x !== s));
  }

  key(e: KeyboardEvent): void {
    const opts = this.options();
    if (e.key === 'ArrowDown' && opts.length) {
      e.preventDefault();
      this.active.set((this.active() + 1) % opts.length);
    } else if (e.key === 'ArrowUp' && opts.length) {
      e.preventDefault();
      this.active.set((this.active() - 1 + opts.length) % opts.length);
    } else if ((e.key === 'Enter' || e.key === ',') && opts.length) {
      e.preventDefault();
      this.pick(opts[Math.min(this.active(), opts.length - 1)]);
    } else if (e.key === 'Backspace' && !this.query() && this.value().length) {
      this.value.update((v) => v.slice(0, -1));
    }
  }
}
