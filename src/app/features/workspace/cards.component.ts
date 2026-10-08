import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';

import { ZardCardImports } from '@/shared/components/card/card.imports';
import { ModelsStore } from '../../core/stores/models.store';
import { FillMeterComponent } from './fill-meter.component';
import { WorkspaceStore } from './workspace.store';

export interface RoleSignals {
  seniority: { label: string; p: number } | null;
  roleFamily: { label: string; p: number } | null;
  skills: { name: string; p: number }[];
}

function top(probabilities: Record<string, number> | undefined): { label: string; p: number } | null {
  const best = Object.entries(probabilities ?? {}).sort((a, b) => b[1] - a[1])[0];
  return best ? { label: best[0], p: best[1] } : null;
}

export function roleSignals(ws: Pick<WorkspaceStore, 'scores' | 'ranked'>, limit = 6): RoleSignals {
  const meta = (q: string) => ws.scores().find((r) => r.target_type === 'meta' && r.question === q)?.probabilities;
  return {
    seniority: top(meta('seniority')),
    roleFamily: top(meta('role_family')),
    skills: (ws.ranked()?.skills ?? [])
      .map(([s, p]) => ({ name: s.name, p }))
      .sort((a, b) => b.p - a.p)
      .slice(0, limit),
  };
}

export function formatDuration(ms: number | null): string {
  if (ms === null) return '—';
  const s = Math.round(ms / 1000);
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
}

@Component({
  selector: 'app-workspace-cards',
  imports: [...ZardCardImports, FillMeterComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid auto-rows-min gap-4 md:grid-cols-3">
      <z-card class="gap-3 py-4">
        <z-card-header><z-card-title class="text-sm">Role signals</z-card-title></z-card-header>
        <z-card-content class="flex flex-col gap-2 text-sm">
          <div class="flex flex-wrap gap-1.5">
            @if (signals().seniority; as s) {
              <span class="bg-secondary rounded-md px-2 py-0.5 text-xs">{{ s.label }} · {{ pct(s.p) }}</span>
            }
            @if (signals().roleFamily; as r) {
              <span class="bg-secondary rounded-md px-2 py-0.5 text-xs">{{ r.label }} · {{ pct(r.p) }}</span>
            }
          </div>
          @for (s of signals().skills; track s.name) {
            <div class="flex items-center gap-2 text-xs">
              <span class="w-24 truncate">{{ s.name }}</span>
              <span class="bg-muted h-1.5 flex-1 overflow-hidden rounded-full">
                <span class="bg-primary block h-full" [style.width.%]="s.p * 100"></span>
              </span>
              <span class="text-muted-foreground w-9 text-right tabular-nums">{{ pct(s.p) }}</span>
            </div>
          } @empty {
            <span class="text-muted-foreground text-xs">No library skills to check.</span>
          }
        </z-card-content>
      </z-card>

      <z-card class="gap-3 py-4">
        <z-card-header><z-card-title class="text-sm">Match summary</z-card-title></z-card-header>
        <z-card-content class="grid grid-cols-2 gap-y-2 text-sm">
          @if (ws.counts(); as c) {
            <span class="text-muted-foreground">Entries</span><span class="tabular-nums">{{ c.entries }} / {{ c.totalEntries }}</span>
            <span class="text-muted-foreground">Bullets</span><span class="tabular-nums">{{ c.bullets }} / {{ c.totalBullets }}</span>
          }
          <span class="text-muted-foreground">Model</span><span class="truncate">{{ model() }}</span>
          <span class="text-muted-foreground">Duration</span><span class="tabular-nums">{{ duration() }}</span>
        </z-card-content>
      </z-card>

      <z-card class="gap-3 py-4">
        <z-card-header><z-card-title class="text-sm">Page fit</z-card-title></z-card-header>
        <z-card-content class="flex flex-col gap-3 text-sm">
          <app-fill-meter [pages]="ws.fit()?.pages ?? null" [fill]="ws.fit()?.fill ?? 0" [overflow]="ws.fit()?.overflow ?? false" [busy]="ws.fitting()" />
          <div class="grid grid-cols-2 gap-2">
            <label class="grid gap-1 text-xs">
              <span class="text-muted-foreground">Template</span>
              <select class="border-input h-8 rounded-md border bg-transparent px-2 text-sm" disabled>
                <option>Classic ATS</option>
              </select>
            </label>
            <label class="grid gap-1 text-xs">
              <span class="text-muted-foreground">Page budget</span>
              <select
                class="border-input h-8 rounded-md border bg-transparent px-2 text-sm"
                [value]="'' + ws.budget()"
                (change)="ws.setBudget(+$any($event.target).value)"
                aria-label="Page budget"
              >
                <option value="1">1 page</option>
                <option value="2">2 pages</option>
              </select>
            </label>
          </div>
        </z-card-content>
      </z-card>
    </div>
  `,
})
export class WorkspaceCardsComponent {
  protected readonly ws = inject(WorkspaceStore);
  private readonly models = inject(ModelsStore);
  protected readonly signals = computed(() => roleSignals(this.ws));
  protected readonly duration = computed(() => formatDuration(this.ws.durationMs()));
  protected readonly model = computed(() => {
    const id = this.ws.run()?.model_id;
    return this.models.models().find((m) => m.spec.id === id)?.spec.label.replace(/\s*\(.*\)$/, '') ?? (id || '—');
  });
  protected pct(p: number): string {
    return `${Math.round(p * 100)}%`;
  }
}
