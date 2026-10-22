import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';

import { ZardBadgeComponent } from '@/shared/components/badge/badge.component';
import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { Company, CompanyProfile, MatchRun } from '../../core/ipc/types';

/** Whether to flag a finished run's company context, and whether Re-score would help. */
export function contextBadge(run: MatchRun | null, company: Company | null): { show: boolean; canRescore: boolean; label: string } {
  const hasProfile = !!company?.researched_at;
  if (run?.status !== 'done') return { show: false, canRescore: false, label: '' };
  if (!run.company_researched_at) return { show: true, canRescore: hasProfile, label: 'Scored without company context' };
  const updated = hasProfile && company!.researched_at !== run.company_researched_at;
  return { show: updated, canRescore: updated, label: updated ? 'Company profile updated since scoring' : '' };
}

/** A refresh overwrites manual edits, so it asks first. */
export const needsRefreshConfirm = (c: Company) => !!c.edited_at;

type TextField = Exclude<keyof CompanyProfile, 'tech_stack' | 'sources'>;
const FIELDS: { key: TextField; label: string }[] = [
  { key: 'summary', label: 'Summary' },
  { key: 'industry', label: 'Industry' },
  { key: 'size', label: 'Size' },
  { key: 'stage', label: 'Stage' },
  { key: 'headquarters', label: 'Headquarters' },
  { key: 'remote_policy', label: 'Remote policy' },
  { key: 'products', label: 'Products' },
];

/** The researched company profile: view, edit, refresh. */
@Component({
  selector: 'app-company-panel',
  imports: [ZardBadgeComponent, ZardButtonComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @let c = company();
    <section class="flex flex-col gap-3 rounded-xl border p-4 text-sm" aria-label="Company profile">
      <div class="flex flex-wrap items-center gap-2">
        <h2 class="font-semibold">{{ c.name }}</h2>
        @if (c.domain) {
          <span class="text-muted-foreground">{{ c.domain }}</span>
        }
        @if (c.edited_at) {
          <z-badge zType="outline">Edited by you</z-badge>
        }
        @if (c.status === 'failed') {
          <z-badge zType="destructive">Research failed</z-badge>
        }
        <span class="text-muted-foreground ml-auto text-xs">{{ c.researched_at ? 'Researched ' + c.researched_at + ' UTC' : 'Not researched yet' }}</span>
      </div>
      @if (c.status === 'failed' && c.last_error) {
        <p class="text-destructive text-xs">{{ c.last_error }}</p>
      }

      @if (editing(); as d) {
        <div class="grid gap-2">
          @for (f of fields; track f.key) {
            <label class="grid gap-1">
              <span class="text-muted-foreground text-xs">{{ f.label }}</span>
              <textarea class="border-input min-h-8 rounded-md border bg-transparent px-2 py-1" rows="1" [value]="d[f.key]" (input)="set(f.key, $any($event.target).value)"></textarea>
            </label>
          }
          <label class="grid gap-1">
            <span class="text-muted-foreground text-xs">Tech stack (comma-separated)</span>
            <input class="border-input h-8 rounded-md border bg-transparent px-2" [value]="d.tech_stack.join(', ')" (input)="setStack($any($event.target).value)" />
          </label>
          <div class="flex gap-2">
            <button z-button zSize="sm" [zLoading]="busy()" (click)="save()">Save</button>
            <button z-button zType="ghost" zSize="sm" (click)="editing.set(null)">Cancel</button>
          </div>
        </div>
      } @else {
        @if (rows().length) {
          <dl class="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
            @for (r of rows(); track r.label) {
              <dt class="text-muted-foreground">{{ r.label }}</dt>
              <dd>{{ r.value }}</dd>
            }
          </dl>
        } @else {
          <p class="text-muted-foreground">No facts yet.</p>
        }
        @if (c.profile.sources.length) {
          <div class="grid gap-1">
            <span class="text-muted-foreground text-xs">Sources</span>
            <ul class="grid gap-0.5 text-xs">
              @for (s of c.profile.sources; track s.url) {
                <li><a [href]="s.url" target="_blank" rel="noopener noreferrer" class="underline">{{ s.title || s.url }}</a></li>
              }
            </ul>
          </div>
        }
        <div class="flex flex-wrap items-center gap-2">
          <button z-button zType="outline" zSize="sm" (click)="edit()">Edit</button>
          @if (confirming()) {
            <span class="text-muted-foreground text-xs">Refreshing replaces your edits.</span>
            <button z-button zType="destructive" zSize="sm" [zLoading]="busy()" (click)="refresh(true)">Refresh anyway</button>
            <button z-button zType="ghost" zSize="sm" (click)="confirming.set(false)">Keep edits</button>
          } @else {
            <button z-button zType="outline" zSize="sm" [zLoading]="busy()" (click)="refresh(false)">Refresh</button>
          }
        </div>
      }
    </section>
  `,
})
export class CompanyPanelComponent {
  readonly company = input.required<Company>();
  readonly changed = output<Company>();
  private readonly toast = inject(ZardSonnerService);
  protected readonly fields = FIELDS;
  protected readonly editing = signal<CompanyProfile | null>(null);
  protected readonly confirming = signal(false);
  protected readonly busy = signal(false);
  protected readonly rows = computed(() => {
    const p = this.company().profile;
    const rows = FIELDS.map((f) => ({ label: f.label, value: p[f.key] }));
    rows.push({ label: 'Tech stack', value: p.tech_stack.join(', ') });
    return rows.filter((r) => r.value.trim());
  });

  protected edit(): void {
    this.editing.set(structuredClone(this.company().profile));
  }

  protected set(key: TextField, value: string): void {
    this.editing.update((d) => (d ? { ...d, [key]: value } : d));
  }

  protected setStack(value: string): void {
    const tech_stack = value.split(',').map((s) => s.trim()).filter(Boolean);
    this.editing.update((d) => (d ? { ...d, tech_stack } : d));
  }

  protected async save(): Promise<void> {
    const d = this.editing();
    if (!d) return;
    await this.run(async () => {
      this.changed.emit(await api.editCompany(this.company().id, d));
      this.editing.set(null);
    }, 'Could not save');
  }

  protected async refresh(confirmed: boolean): Promise<void> {
    if (!confirmed && needsRefreshConfirm(this.company())) {
      this.confirming.set(true);
      return;
    }
    this.confirming.set(false);
    await this.run(async () => this.changed.emit(await api.refreshCompany(this.company().id)), 'Could not refresh');
  }

  private async run(f: () => Promise<void>, title: string): Promise<void> {
    this.busy.set(true);
    try {
      await f();
    } catch (e) {
      this.toast.error(title, { description: messageFor(e) });
    } finally {
      this.busy.set(false);
    }
  }
}
