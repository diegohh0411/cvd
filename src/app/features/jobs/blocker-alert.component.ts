import { ChangeDetectionStrategy, Component, input } from '@angular/core';
import { RouterLink } from '@angular/router';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import type { Blocker } from './match-readiness';

@Component({
  selector: 'app-blocker-alert',
  imports: [RouterLink, ZardButtonComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div role="status" class="bg-muted/50 flex flex-wrap items-center justify-between gap-3 rounded-lg border px-4 py-3 text-sm">
      <span>{{ blocker().message }}</span>
      <a z-button zType="outline" zSize="sm" [routerLink]="blocker().link" [queryParams]="blocker().queryParams">{{ blocker().action }}</a>
    </div>
  `,
})
export class BlockerAlertComponent {
  readonly blocker = input.required<Blocker>();
}
