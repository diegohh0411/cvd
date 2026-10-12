import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucideBriefcase, lucideChevronDown, lucideChevronUp, lucideSparkles, lucideWrench } from '@ng-icons/lucide';

import { ZardBadgeComponent } from '@/shared/components/badge/badge.component';
import { ZardCardImports } from '@/shared/components/card/card.imports';
import { ModelsStore } from '../../core/stores/models.store';
import { FillMeterComponent } from './fill-meter.component';
import { WorkspaceStore } from './workspace.store';

export interface RoleSignals {
  seniority: { label: string; p: number } | null;
  roleFamily: { label: string; p: number } | null;
  skills: { name: string; p: number }[];
  tags?: string[];
}

function top(probabilities: Record<string, number> | undefined): { label: string; p: number } | null {
  const best = Object.entries(probabilities ?? {}).sort((a, b) => b[1] - a[1])[0];
  return best ? { label: best[0], p: best[1] } : null;
}

export function roleSignals(ws: Pick<WorkspaceStore, 'scores' | 'ranked'>, limit?: number): RoleSignals {
  const meta = (q: string) => ws.scores().find((r) => r.target_type === 'meta' && r.question === q)?.probabilities;
  const allSkills = (ws.ranked()?.skills ?? [])
    .map(([s, p]) => ({ name: s.name, p }))
    .sort((a, b) => b.p - a.p);

  return {
    seniority: top(meta('seniority')),
    roleFamily: top(meta('role_family')),
    skills: typeof limit === 'number' ? allSkills.slice(0, limit) : allSkills,
    tags: ws.ranked()?.tags ?? [],
  };
}

export function formatDuration(ms: number | null): string {
  if (ms === null) return '—';
  const s = Math.round(ms / 1000);
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`;
}

@Component({
  selector: 'app-workspace-cards',
  imports: [...ZardCardImports, ZardBadgeComponent, NgIcon, FillMeterComponent],
  viewProviders: [provideIcons({ lucideBriefcase, lucideChevronDown, lucideChevronUp, lucideSparkles, lucideWrench })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid auto-rows-min gap-4 lg:grid-cols-3">
      <!-- Analysis & Fit Card (spans 2 columns on large screens) -->
      <z-card class="gap-3 py-4 lg:col-span-2">
        <z-card-header class="flex flex-row items-center justify-between space-y-0 pb-1">
          <z-card-title class="flex items-center gap-1.5 text-sm font-medium" zTitle="Analysis & Fit">
            <ng-icon name="lucideSparkles" class="text-primary size-4" />
            <span>Analysis & Fit</span>
          </z-card-title>
          <span class="text-muted-foreground text-xs">AI classification & library alignment</span>
        </z-card-header>
        <z-card-content class="pt-2 text-sm">
          <div class="grid gap-6 md:grid-cols-2 md:divide-x md:divide-border">
            <!-- Left Column: Role Understanding -->
            <div class="flex flex-col gap-3.5">
              <div class="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                <ng-icon name="lucideBriefcase" class="size-3.5" />
                <span>Role Understanding</span>
              </div>

              <!-- Classification Chips -->
              <div class="flex flex-col gap-2">
                <span class="text-xs text-muted-foreground">Classification</span>
                <div class="flex flex-wrap items-center gap-1.5">
                  @if (signals().seniority; as s) {
                    <z-badge zType="secondary" class="font-medium">
                      {{ s.label }} · <span class="tabular-nums">{{ pct(s.p) }}</span>
                    </z-badge>
                  }
                  @if (signals().roleFamily; as r) {
                    <z-badge zType="secondary" class="font-medium">
                      {{ r.label }} · <span class="tabular-nums">{{ pct(r.p) }}</span>
                    </z-badge>
                  }
                  @if (!signals().seniority && !signals().roleFamily) {
                    <span class="text-xs text-muted-foreground italic">No role signals detected</span>
                  }
                </div>
              </div>

              <!-- Role Focus Tags (from JD analysis) -->
              <div class="flex flex-col gap-2">
                <span class="text-xs text-muted-foreground">Role Focus Areas</span>
                <div class="flex flex-wrap gap-1.5" aria-label="Role focus tags">
                  @for (tag of focusTags(); track tag) {
                    <z-badge zType="outline" class="text-xs font-normal">
                      #{{ tag }}
                    </z-badge>
                  } @empty {
                    <span class="text-xs text-muted-foreground italic">No focus topics extracted</span>
                  }
                </div>
              </div>
            </div>

            <!-- Right Column: Library Match -->
            <div class="flex flex-col gap-3.5 md:ps-6">
              <div class="flex items-center justify-between text-xs font-semibold uppercase tracking-wider text-muted-foreground">
                <div class="flex items-center gap-1.5">
                  <ng-icon name="lucideWrench" class="size-3.5" />
                  <span>Library Match</span>
                </div>
                @if (signals().skills.length > 0) {
                  <span class="text-[11px] font-normal normal-case text-muted-foreground">
                    {{ signals().skills.length }} library skills checked
                  </span>
                }
              </div>

              @if (signals().skills.length > 0) {
                <!-- Strong Matches -->
                @if (strongSkills().length > 0) {
                  <div class="flex flex-col gap-1.5">
                    <span class="text-xs font-medium text-foreground">High relevance (&ge;70%)</span>
                    <div class="flex flex-wrap gap-1.5">
                      @for (s of strongSkills(); track s.name) {
                        <z-badge zType="secondary" class="gap-1 border-primary/20 bg-primary/10 text-primary hover:bg-primary/15 font-medium">
                          <span class="size-1.5 rounded-full bg-primary inline-block"></span>
                          <span>{{ s.name }}</span>
                          <span class="opacity-75 tabular-nums text-[10px]">{{ pct(s.p) }}</span>
                        </z-badge>
                      }
                    </div>
                  </div>
                }

                <!-- Moderate Matches -->
                @if (visibleModerateSkills().length > 0) {
                  <div class="flex flex-col gap-1.5">
                    <span class="text-xs text-muted-foreground">Secondary relevance</span>
                    <div class="flex flex-wrap gap-1.5">
                      @for (s of visibleModerateSkills(); track s.name) {
                        <z-badge zType="outline" class="gap-1 text-muted-foreground hover:text-foreground">
                          <span class="size-1.5 rounded-full bg-muted-foreground/40 inline-block"></span>
                          <span>{{ s.name }}</span>
                          <span class="tabular-nums text-[10px]">{{ pct(s.p) }}</span>
                        </z-badge>
                      }
                    </div>
                  </div>
                }

                @if (hiddenModerateCount() > 0 || showAllSkills()) {
                  <button
                    type="button"
                    class="mt-1 flex w-fit items-center gap-1 text-xs text-muted-foreground hover:text-foreground transition-colors"
                    (click)="showAllSkills.set(!showAllSkills())"
                  >
                    @if (showAllSkills()) {
                      <ng-icon name="lucideChevronUp" class="size-3" />
                      <span>Show fewer</span>
                    } @else {
                      <ng-icon name="lucideChevronDown" class="size-3" />
                      <span>Show {{ hiddenModerateCount() }} more</span>
                    }
                  </button>
                }
              } @else {
                <span class="text-xs text-muted-foreground italic">No library skills to check.</span>
              }
            </div>
          </div>
        </z-card-content>
      </z-card>

      <!-- Right Column Stack: Page Fit + Match Summary -->
      <div class="flex flex-col gap-4">
        <!-- Page fit -->
        <z-card class="gap-3 py-4">
          <z-card-header><z-card-title class="text-sm" zTitle="Page fit" /></z-card-header>
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

        <!-- Match summary -->
        <z-card class="gap-3 py-4">
          <z-card-header><z-card-title class="text-sm" zTitle="Match summary" /></z-card-header>
          <z-card-content class="grid grid-cols-2 gap-y-2 text-sm">
            @if (ws.counts(); as c) {
              <span class="text-muted-foreground">Entries</span><span class="tabular-nums">{{ c.entries }} / {{ c.totalEntries }}</span>
              <span class="text-muted-foreground">Bullets</span><span class="tabular-nums">{{ c.bullets }} / {{ c.totalBullets }}</span>
            }
            <span class="text-muted-foreground">Model</span><span class="truncate">{{ model() }}</span>
            <span class="text-muted-foreground">Duration</span><span class="tabular-nums">{{ duration() }}</span>
          </z-card-content>
        </z-card>
      </div>
    </div>
  `,
})
export class WorkspaceCardsComponent {
  protected readonly ws = inject(WorkspaceStore);
  private readonly models = inject(ModelsStore);

  protected readonly showAllSkills = signal(false);

  protected readonly signals = computed(() => roleSignals(this.ws));
  protected readonly focusTags = computed(() => this.ws.ranked()?.tags ?? []);
  protected readonly duration = computed(() => formatDuration(this.ws.durationMs()));
  protected readonly model = computed(() => {
    const id = this.ws.run()?.model_id;
    return this.models.models().find((m) => m.spec.id === id)?.spec.label.replace(/\s*\(.*\)$/, '') ?? (id || '—');
  });

  protected readonly strongSkills = computed(() =>
    this.signals().skills.filter((s) => s.p >= 0.7),
  );

  protected readonly moderateSkills = computed(() =>
    this.signals().skills.filter((s) => s.p < 0.7),
  );

  protected readonly visibleModerateSkills = computed(() => {
    if (this.showAllSkills()) return this.moderateSkills();
    return this.moderateSkills().slice(0, 4);
  });

  protected readonly hiddenModerateCount = computed(() => {
    const total = this.moderateSkills().length;
    return Math.max(0, total - 4);
  });

  protected pct(p: number): string {
    return `${Math.round(p * 100)}%`;
  }
}

