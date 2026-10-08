import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

import { ZardBadgeComponent } from '@/shared/components/badge/badge.component';
import type { MatchRun } from '../../core/ipc/types';

@Component({
  selector: 'app-run-status-badge',
  imports: [ZardBadgeComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `<z-badge [zType]="view().type">{{ view().label }}</z-badge>`,
})
export class RunStatusBadgeComponent {
  readonly run = input<MatchRun | undefined>();
  protected readonly view = computed(() => {
    switch (this.run()?.status) {
      case 'done':
        return { label: 'Matched', type: 'secondary' as const };
      case 'running':
        return { label: 'Matching…', type: 'default' as const };
      case 'cancelled':
        return { label: 'Cancelled', type: 'outline' as const };
      case 'failed':
        return { label: 'Failed', type: 'destructive' as const };
      default:
        return { label: 'Not matched', type: 'outline' as const };
    }
  });
}
