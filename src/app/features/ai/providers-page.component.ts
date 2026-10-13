import { ChangeDetectionStrategy, Component } from '@angular/core';

import { PageHeaderComponent } from '../../shell/page-header.component';
import { LocalProviderComponent } from './local-provider.component';
import { ProviderKeyComponent } from './provider-key.component';

@Component({
  selector: 'app-providers-page',
  imports: [PageHeaderComponent, LocalProviderComponent, ProviderKeyComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'AI' }, { label: 'Providers' }]" />
    <div class="mx-auto flex w-full max-w-3xl flex-1 flex-col gap-3 p-4">
      <div>
        <h2 class="font-semibold">Providers</h2>
        <p class="text-muted-foreground text-sm">Where models run, and the keys for online services.</p>
      </div>
      <div class="rounded-lg border p-4"><app-local-provider /></div>
      <div class="rounded-lg border p-4"><app-provider-key provider="openrouter" /></div>
      <div class="rounded-lg border p-4"><app-provider-key provider="subq" /></div>
      <div class="rounded-lg border p-4"><app-provider-key provider="exa" /></div>
    </div>
  `,
})
export class ProvidersPageComponent {}
