import { ChangeDetectionStrategy, Component, computed, DestroyRef, inject, input, output, signal } from '@angular/core';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucideCheck, lucideLoader } from '@ng-icons/lucide';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardProgressComponent } from '@/shared/components/progress/progress.component';
import type { MatchStage } from '../../core/ipc/types';
import { ResearchProgressComponent } from '../companies/research-progress.component';
import type { ResearchState } from './workspace.store';

export const STEPS = ['Reading JD', 'Scoring entries and bullets', 'Fitting page'] as const;

/** Index of the active step and its progress (0–100) for a progress event. */
export function stepState(p: MatchStage | null): { index: number; percent: number | null; detail: string } {
  switch (p?.stage) {
    case 'scoring':
      return { index: 1, percent: pct(p.done, p.total), detail: `${p.done}/${p.total}` };
    case 'done':
      return { index: 2, percent: null, detail: '' };
    default:
      return { index: 0, percent: null, detail: '' };
  }
}

const pct = (done: number, total: number) => (total ? Math.round((done / total) * 100) : 0);

@Component({
  selector: 'app-matching-stepper',
  imports: [ZardButtonComponent, ZardProgressComponent, NgIcon, ResearchProgressComponent],
  viewProviders: [provideIcons({ lucideCheck, lucideLoader })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="mx-auto flex w-full max-w-md flex-col gap-6 rounded-xl border p-6">
      <div>
        <h2 class="font-semibold">Matching your library</h2>
        <p class="text-muted-foreground text-sm">Runs locally. Usually 30–90 seconds. <span class="tabular-nums">{{ elapsed() }}s</span></p>
      </div>
      @if (research(); as r) {
        <app-research-progress [state]="r" [companyName]="companyName()" (skip)="skipResearch.emit($event)" />
      }
      <ol class="flex flex-col gap-3" aria-live="polite">
        @for (s of steps; track s; let i = $index) {
          <li class="flex flex-col gap-1.5">
            <div class="flex items-center gap-2 text-sm" [class.text-muted-foreground]="i > state().index">
              @if (i < state().index) {
                <ng-icon name="lucideCheck" class="size-4 text-emerald-600" />
              } @else if (i === state().index) {
                <ng-icon name="lucideLoader" class="size-4 animate-spin" />
              } @else {
                <span class="size-4 rounded-full border"></span>
              }
              <span [class.font-medium]="i === state().index">{{ s }}</span>
              @if (i === state().index && state().detail) {
                <span class="text-muted-foreground ml-auto text-xs tabular-nums">{{ state().detail }}</span>
              }
            </div>
            @if (i === state().index && state().percent !== null) {
              <z-progress [value]="state().percent!" class="h-1.5" />
            }
          </li>
        }
      </ol>
      <button z-button zType="outline" (click)="cancel.emit()">Cancel</button>
    </div>
  `,
})
export class MatchingStepperComponent {
  readonly progress = input<MatchStage | null>(null);
  readonly startedAt = input<number | null>(null);
  readonly research = input<ResearchState | null>(null);
  readonly companyName = input('the company');
  readonly cancel = output<void>();
  readonly skipResearch = output<'cancel' | 'background'>();
  protected readonly steps = STEPS;
  protected readonly state = computed(() => stepState(this.progress()));
  private readonly now = signal(Date.now());
  protected readonly elapsed = computed(() => {
    const s = this.startedAt();
    return s ? Math.max(0, Math.round((this.now() - s) / 1000)) : 0;
  });

  constructor() {
    const t = setInterval(() => this.now.set(Date.now()), 1000);
    inject(DestroyRef).onDestroy(() => clearInterval(t));
  }
}
