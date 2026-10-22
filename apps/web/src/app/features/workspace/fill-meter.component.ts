import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

/** "1 page · 92% full", or an overflow warning. `fill` is the last page's fill, 0–1. */
export function fillText(pages: number | null, fill: number, overflow: boolean): string {
  if (pages === null) return 'Not fitted yet';
  const p = `${pages} page${pages === 1 ? '' : 's'}`;
  return overflow ? `${p} · overflow` : `${p} · ${Math.round(Math.max(0, Math.min(1, fill)) * 100)}% full`;
}

@Component({
  selector: 'app-fill-meter',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex flex-col gap-1.5" aria-live="polite">
      <div class="flex items-baseline justify-between text-sm">
        <span [class.text-destructive]="overflow()" class="font-medium tabular-nums" data-testid="fill-text">{{ text() }}</span>
        @if (busy()) {
          <span class="text-muted-foreground text-xs">fitting…</span>
        }
      </div>
      <div
        class="bg-muted h-2 overflow-hidden rounded-full"
        role="meter"
        aria-label="Page fill"
        aria-valuemin="0"
        aria-valuemax="100"
        [attr.aria-valuenow]="percent()"
      >
        <div class="h-full rounded-full transition-[width]" [class]="overflow() ? 'bg-destructive' : 'bg-primary'" [style.width.%]="percent()"></div>
      </div>
    </div>
  `,
})
export class FillMeterComponent {
  readonly pages = input<number | null>(null);
  readonly fill = input(0);
  readonly overflow = input(false);
  readonly busy = input(false);
  protected readonly text = computed(() => fillText(this.pages(), this.fill(), this.overflow()));
  protected readonly percent = computed(() => (this.overflow() ? 100 : Math.round(Math.max(0, Math.min(1, this.fill())) * 100)));
}
