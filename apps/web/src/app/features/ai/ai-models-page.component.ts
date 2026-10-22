import { ChangeDetectionStrategy, Component } from '@angular/core';

import { PageHeaderComponent } from '../../shell/page-header.component';
import { LocalProviderComponent } from './local-provider.component';
import { ProviderConnectionsComponent } from './provider-connections.component';
import { ProviderKeyComponent } from './provider-key.component';
import { PROVIDERS } from './provider-registry';
import { RoleCardComponent } from './role-card.component';

@Component({
  selector: 'app-ai-models-page',
  imports: [PageHeaderComponent, RoleCardComponent, LocalProviderComponent, ProviderConnectionsComponent, ProviderKeyComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'AI' }, { label: 'AI Models' }]" />
    <div class="mx-auto flex w-full max-w-3xl flex-1 flex-col gap-4 p-4">
      <div>
        <h2 class="font-semibold">AI Models</h2>
        <p class="text-muted-foreground text-sm">Which model does each job, and the connections it runs on.</p>
      </div>
      <app-role-card role="system-one" />
      <app-role-card role="research" />

      <div class="mt-2" id="connections">
        <h2 class="font-semibold">Connections</h2>
        <p class="text-muted-foreground text-sm">Where models run, and the keys for online services.</p>
      </div>
      <div class="grid gap-3" data-testid="connections">
        <div class="rounded-lg border p-4"><app-local-provider /></div>
        @for (p of modelProviders; track p.id) {
          <div class="rounded-lg border p-4"><app-provider-connections [provider]="p.id" /></div>
        }
        <div class="rounded-lg border p-4"><app-provider-key provider="exa" /></div>
      </div>
    </div>
  `,
})
export class AiModelsPageComponent {
  protected readonly modelProviders = PROVIDERS.filter((p) => p.roles.length > 0 && p.id !== 'localhost');
}
