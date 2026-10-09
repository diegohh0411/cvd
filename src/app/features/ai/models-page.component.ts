import { ChangeDetectionStrategy, Component, inject, OnInit } from '@angular/core';
import { RouterLink } from '@angular/router';

import { EngineStore } from '../../core/stores/engine.store';
import { ModelsStore } from '../../core/stores/models.store';
import { PageHeaderComponent } from '../../shell/page-header.component';
import { ModelCardsComponent } from './model-cards.component';

@Component({
  selector: 'app-ai-models-page',
  imports: [PageHeaderComponent, ModelCardsComponent, RouterLink],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'AI' }, { label: 'Models' }]" />
    <div class="mx-auto flex w-full max-w-3xl flex-1 flex-col gap-4 p-4">
      <div>
        <h2 class="font-semibold">Local models</h2>
        <p class="text-muted-foreground text-sm">
          Models run entirely on this computer; downloading is the only time the app goes online.
          @if (models.ramGb(); as ram) {
            This computer has {{ ram }} GB of RAM.
          }
          Choose which one does each job on <a routerLink="/ai/roles" class="underline">Roles</a>.
        </p>
      </div>
      <app-model-cards />
    </div>
  `,
})
export class AiModelsPageComponent implements OnInit {
  protected readonly models = inject(ModelsStore);
  private readonly engine = inject(EngineStore);

  ngOnInit(): void {
    void this.models.load();
    void this.engine.refresh();
  }
}
