import { ChangeDetectionStrategy, Component, computed, DestroyRef, inject, input, output, signal } from '@angular/core';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucideLoader } from '@ng-icons/lucide';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import type { ResearchStep } from '../../core/ipc/types';
import type { ResearchState } from '../workspace/workspace.store';

const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;

/** "Researching Acme… step 7/25 · 1:42" */
export function researchLine(name: string, round: { n: number; max: number } | null, elapsedSec: number): string {
  const step = round ? ` step ${round.n}/${round.max}` : '';
  return `Researching ${name}…${step} · ${clock(Math.max(0, Math.round(elapsedSec)))}`;
}

export function stepLine(step: Exclude<ResearchStep, { kind: 'round' }>): string {
  return step.kind === 'searching' ? `Searching “${step.query}”` : `Reading ${step.domain}`;
}

/** Live company research inside the matching card, with the two ways to stop waiting. */
@Component({
  selector: 'app-research-progress',
  imports: [ZardButtonComponent, NgIcon],
  viewProviders: [provideIcons({ lucideLoader })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex flex-col gap-3" aria-live="polite">
      <div class="flex items-center gap-2 text-sm font-medium">
        <ng-icon name="lucideLoader" class="size-4 animate-spin" />
        <span class="tabular-nums">{{ line() }}</span>
      </div>
      <p class="text-muted-foreground text-xs">Matching waits for this so the model knows the company. Profiles are cached for 30 days.</p>
      @if (state().log.length) {
        <ul class="text-muted-foreground flex flex-col gap-0.5 font-mono text-xs">
          @for (l of state().log; track $index) {
            <li class="truncate">{{ l }}</li>
          }
        </ul>
      }
      <div class="flex flex-wrap gap-2">
        <button z-button zType="outline" zSize="sm" (click)="skip.emit('cancel')">Cancel research &amp; proceed</button>
        <button z-button zType="secondary" zSize="sm" (click)="skip.emit('background')">Continue in background &amp; proceed</button>
      </div>
    </div>
  `,
})
export class ResearchProgressComponent {
  readonly state = input.required<ResearchState>();
  readonly companyName = input('the company');
  readonly skip = output<'cancel' | 'background'>();
  private readonly now = signal(Date.now());
  protected readonly line = computed(() =>
    researchLine(this.companyName(), this.state().round, (this.now() - this.state().startedAt) / 1000),
  );

  constructor() {
    const t = setInterval(() => this.now.set(Date.now()), 1000);
    inject(DestroyRef).onDestroy(() => clearInterval(t));
  }
}
