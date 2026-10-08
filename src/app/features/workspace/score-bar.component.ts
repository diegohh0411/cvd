import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { ZardHoverCardComponent, ZardHoverCardDirective } from '@/shared/components/hover-card/hover-card.component';
import type { ScoreRow } from '../../core/ipc/types';
import { SCORE_FLOOR, SCORE_MAX } from './selection.model';

export interface BreakdownRow {
  question: string;
  label: string;
  bars: { key: string; p: number }[];
}

const QUESTION_LABELS: Record<string, string> = {
  relevance: 'Relevance (0–4 rubric)',
  seniority_fit: 'Shows the needed seniority',
};

/** Score rows for one target as labelled probability bars, rubric keys in numeric order. */
export function breakdown(rows: readonly ScoreRow[]): BreakdownRow[] {
  return rows.map((r) => ({
    question: r.question,
    label: QUESTION_LABELS[r.question] ?? (r.question.startsWith('bullet:') ? 'Support for the role (0–4 rubric)' : r.question),
    bars: Object.entries(r.probabilities)
      .map(([key, p]) => ({ key, p }))
      .sort((a, b) => (isNaN(+a.key) || isNaN(+b.key) ? b.p - a.p : +a.key - +b.key)),
  }));
}

export function band(score: number): 'high' | 'mid' | 'low' {
  return score >= 3 ? 'high' : score >= SCORE_FLOOR ? 'mid' : 'low';
}

/** Score bar; hover or focus shows the rubric probability breakdown (spec §7). */
@Component({
  selector: 'app-score-bar',
  imports: [ZardHoverCardDirective, ZardHoverCardComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <button
      type="button"
      class="focus-visible:ring-ring/50 flex w-24 shrink-0 items-center gap-2 rounded outline-none focus-visible:ring-[3px]"
      [zHoverCard]="detail"
      [zOpenDelay]="250"
      zPlacement="left"
      [attr.aria-label]="'Score ' + score().toFixed(1) + ' of 4'"
    >
      <span class="bg-muted h-1.5 flex-1 overflow-hidden rounded-full">
        <span class="block h-full rounded-full" [class]="color()" [style.width.%]="width()"></span>
      </span>
      <span class="text-muted-foreground w-6 text-right text-xs tabular-nums">{{ score().toFixed(1) }}</span>
    </button>
    <ng-template #detail>
      <z-hover-card class="w-64">
        @for (row of rows(); track row.question) {
          <div class="mb-2 last:mb-0">
            <div class="mb-1 text-xs font-medium">{{ row.label }}</div>
            @for (b of row.bars; track b.key) {
              <div class="flex items-center gap-2 text-xs">
                <span class="text-muted-foreground w-8 truncate">{{ b.key }}</span>
                <span class="bg-muted h-1.5 flex-1 overflow-hidden rounded-full">
                  <span class="bg-primary block h-full" [style.width.%]="b.p * 100"></span>
                </span>
                <span class="text-muted-foreground w-9 text-right tabular-nums">{{ (b.p * 100).toFixed(0) }}%</span>
              </div>
            }
          </div>
        } @empty {
          <p class="text-muted-foreground text-xs">No breakdown stored for this item.</p>
        }
      </z-hover-card>
    </ng-template>
  `,
})
export class ScoreBarComponent {
  readonly score = input.required<number>();
  readonly scores = input<readonly ScoreRow[]>([]);
  protected readonly width = computed(() => Math.max(0, Math.min(1, this.score() / SCORE_MAX)) * 100);
  protected readonly color = computed(
    () => ({ high: 'bg-emerald-500', mid: 'bg-amber-500', low: 'bg-muted-foreground/50' })[band(this.score())],
  );
  protected readonly rows = computed(() => breakdown(this.scores()));
}
