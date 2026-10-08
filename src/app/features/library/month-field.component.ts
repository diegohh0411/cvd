import { ChangeDetectionStrategy, Component, computed, input, model } from '@angular/core';

import type { CalendarValue } from '@/shared/components/calendar/calendar.types';
import { ZardDatePickerComponent } from '@/shared/components/date-picker/date-picker.component';

/** `2023-03` → 1 Mar 2023 (local); null for anything else. */
export function monthToDate(ym: string | null): Date | null {
  const m = /^(\d{4})-(\d{2})$/.exec(ym ?? '');
  return m ? new Date(Number(m[1]), Number(m[2]) - 1, 1) : null;
}

export function dateToMonth(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`;
}

/**
 * A `YYYY-MM` field over ZardUI's date picker (WebKit has no `<input type="month">`).
 * The day picked is ignored; entries only store the month.
 */
@Component({
  selector: 'app-month-field',
  imports: [ZardDatePickerComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <z-date-picker
      class="w-full"
      [zId]="fieldId()"
      [value]="date()"
      (dateChange)="pick($event)"
      [disabled]="disabled()"
      zFormat="MMMM yyyy"
      zCaptionLayout="dropdown"
      [zPlaceholder]="placeholder()"
    />
  `,
})
export class MonthFieldComponent {
  readonly value = model<string | null>(null);
  readonly fieldId = input('');
  readonly disabled = input(false);
  readonly placeholder = input('Pick a month');
  protected readonly date = computed(() => monthToDate(this.value()));

  pick(v: CalendarValue): void {
    if (v instanceof Date) this.value.set(dateToMonth(v));
  }
}
