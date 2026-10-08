import { ChangeDetectionStrategy, Component, input } from '@angular/core';

import type { StatusColor } from '../../core/ipc/types';

/** Notion-like label styles per palette color (full class strings for Tailwind). */
export const PILL_CLASSES: Record<StatusColor, string> = {
  gray: 'bg-zinc-200/70 text-zinc-700 dark:bg-zinc-700/60 dark:text-zinc-200',
  brown: 'bg-stone-300/60 text-stone-800 dark:bg-stone-600/50 dark:text-stone-200',
  orange: 'bg-orange-100 text-orange-800 dark:bg-orange-900/50 dark:text-orange-200',
  yellow: 'bg-amber-100 text-amber-800 dark:bg-amber-900/50 dark:text-amber-200',
  green: 'bg-emerald-100 text-emerald-800 dark:bg-emerald-900/50 dark:text-emerald-200',
  blue: 'bg-sky-100 text-sky-800 dark:bg-sky-900/50 dark:text-sky-200',
  purple: 'bg-violet-100 text-violet-800 dark:bg-violet-900/50 dark:text-violet-200',
  pink: 'bg-pink-100 text-pink-800 dark:bg-pink-900/50 dark:text-pink-200',
  red: 'bg-red-100 text-red-800 dark:bg-red-900/50 dark:text-red-200',
};

export const SWATCH_CLASSES: Record<StatusColor, string> = {
  gray: 'bg-zinc-400',
  brown: 'bg-stone-500',
  orange: 'bg-orange-400',
  yellow: 'bg-amber-400',
  green: 'bg-emerald-500',
  blue: 'bg-sky-500',
  purple: 'bg-violet-500',
  pink: 'bg-pink-400',
  red: 'bg-red-500',
};

@Component({
  selector: 'app-status-pill',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'inline-flex min-w-0' },
  template: `
    <span
      class="inline-flex max-w-full items-center truncate rounded px-1.5 py-0.5 text-xs leading-4 font-medium"
      [class]="classes[color()]"
      >{{ name() }}</span
    >
  `,
})
export class StatusPillComponent {
  readonly name = input.required<string>();
  readonly color = input.required<StatusColor>();
  protected readonly classes = PILL_CLASSES;
}
