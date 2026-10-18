import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardInputComponent } from '@/shared/components/input/input.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { ApplyDefaults, ApplyFact } from '../../core/ipc/types';

export const EMPTY_DEFAULTS: ApplyDefaults = {
  first_name: '',
  last_name: '',
  city: '',
  region: '',
  country: '',
  authorized_to_work: null,
  needs_sponsorship: null,
  willing_to_relocate: null,
  notice_period: '',
  earliest_start: '',
  salary_expectation: '',
};

type Flag = 'authorized_to_work' | 'needs_sponsorship' | 'willing_to_relocate';
type Text = Exclude<keyof ApplyDefaults, Flag>;

/** `null` ↔ '' so a native select can show "Ask me". */
export const flagToValue = (v: boolean | null) => (v === null ? '' : v ? 'yes' : 'no');
export const valueToFlag = (v: string): boolean | null => (v === 'yes' ? true : v === 'no' ? false : null);

const TEXT_FIELDS: { key: Text; label: string; placeholder: string }[] = [
  { key: 'first_name', label: 'First name', placeholder: 'From your full name' },
  { key: 'last_name', label: 'Last name', placeholder: 'From your full name' },
  { key: 'city', label: 'City', placeholder: 'Monterrey' },
  { key: 'region', label: 'State / region', placeholder: 'Nuevo León' },
  { key: 'country', label: 'Country', placeholder: 'Mexico' },
  { key: 'notice_period', label: 'Notice period', placeholder: '2 weeks' },
  { key: 'earliest_start', label: 'Earliest start', placeholder: 'Immediately' },
  { key: 'salary_expectation', label: 'Salary expectation', placeholder: 'Leave empty to always answer yourself' },
];

const FLAG_FIELDS: { key: Flag; label: string }[] = [
  { key: 'authorized_to_work', label: 'Authorized to work where you apply' },
  { key: 'needs_sponsorship', label: 'Needs visa sponsorship' },
  { key: 'willing_to_relocate', label: 'Willing to relocate' },
];

@Component({
  selector: 'app-apply-defaults',
  imports: [FormsModule, ZardButtonComponent, ZardInputComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="grid gap-3">
      <div>
        <h2 class="font-semibold">Application defaults</h2>
        <p class="text-muted-foreground text-sm">Answers auto-apply may use on job forms. Anything left empty is always left for you to fill.</p>
      </div>
      <div class="grid gap-3 sm:grid-cols-2">
        @for (f of textFields; track f.key) {
          <div class="grid gap-1.5">
            <label class="text-sm font-medium" [for]="'ad-' + f.key">{{ f.label }}</label>
            <input z-input [id]="'ad-' + f.key" [ngModel]="draft()[f.key]" (ngModelChange)="set(f.key, $event)" [placeholder]="f.placeholder" />
          </div>
        }
        @for (f of flagFields; track f.key) {
          <div class="grid gap-1.5">
            <label class="text-sm font-medium" [for]="'ad-' + f.key">{{ f.label }}</label>
            <select
              [id]="'ad-' + f.key"
              class="border-input h-8 rounded-md border bg-transparent px-2 text-sm"
              [value]="flagValue(f.key)"
              (change)="set(f.key, toFlag($any($event.target).value))"
            >
              <option value="">Ask me</option>
              <option value="yes">Yes</option>
              <option value="no">No</option>
            </select>
          </div>
        }
      </div>
      <div><button z-button zType="outline" (click)="save()" [zLoading]="saving()">Save defaults</button></div>
      @if (facts().length) {
        <details class="text-sm">
          <summary class="text-muted-foreground cursor-pointer">What auto-apply can fill ({{ facts().length }})</summary>
          <dl class="mt-2 grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1">
            @for (f of facts(); track f.key) {
              <dt class="text-muted-foreground font-mono text-xs leading-5">{{ f.key }}</dt>
              <dd class="truncate">{{ f.variants.join(' · ') }}</dd>
            }
          </dl>
        </details>
      }
    </section>
  `,
})
export class ApplyDefaultsComponent implements OnInit {
  private readonly toast = inject(ZardSonnerService);
  protected readonly textFields = TEXT_FIELDS;
  protected readonly flagFields = FLAG_FIELDS;
  protected readonly draft = signal<ApplyDefaults>(EMPTY_DEFAULTS);
  protected readonly facts = signal<ApplyFact[]>([]);
  protected readonly saving = signal(false);
  protected readonly toFlag = valueToFlag;

  async ngOnInit(): Promise<void> {
    try {
      this.draft.set({ ...EMPTY_DEFAULTS, ...(await api.getApplyDefaults()) });
      await this.refreshFacts();
    } catch (e) {
      this.toast.error('Could not load application defaults', { description: messageFor(e) });
    }
  }

  protected flagValue(key: Flag): string {
    return flagToValue(this.draft()[key]);
  }

  protected set<K extends keyof ApplyDefaults>(key: K, value: ApplyDefaults[K]): void {
    this.draft.update((d) => ({ ...d, [key]: value }));
  }

  async save(): Promise<void> {
    this.saving.set(true);
    try {
      await api.saveApplyDefaults(this.draft());
      await this.refreshFacts();
      this.toast.success('Application defaults saved');
    } catch (e) {
      this.toast.error('Could not save', { description: messageFor(e) });
    } finally {
      this.saving.set(false);
    }
  }

  /** Called by the page after the profile itself is saved, since facts read both. */
  async refreshFacts(): Promise<void> {
    this.facts.set(await api.applyFacts());
  }
}
