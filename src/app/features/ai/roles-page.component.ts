import { ChangeDetectionStrategy, Component } from '@angular/core';

import { PageHeaderComponent } from '../../shell/page-header.component';
import { RoleCardComponent } from './role-card.component';

@Component({
  selector: 'app-roles-page',
  imports: [PageHeaderComponent, RoleCardComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'AI' }, { label: 'Roles' }]" />
    <div class="mx-auto flex w-full max-w-3xl flex-1 flex-col gap-4 p-4">
      <div>
        <h2 class="font-semibold">Roles</h2>
        <p class="text-muted-foreground text-sm">Which model does each job, and where it runs.</p>
      </div>
      <app-role-card role="system-one" />
      <app-role-card role="research" />
    </div>
  `,
})
export class RolesPageComponent {}
