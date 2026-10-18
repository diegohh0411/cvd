import { ChangeDetectionStrategy, Component, computed, inject, OnInit } from '@angular/core';
import { RouterLink } from '@angular/router';

import { EngineStore } from '../../core/stores/engine.store';
import { ModelsStore } from '../../core/stores/models.store';

/** The bundled llama.cpp sidecar: no key, just what it can run here. */
@Component({
  selector: 'app-local-provider',
  imports: [RouterLink],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid gap-2 text-sm" data-testid="local-provider">
      <span class="font-medium" data-testid="provider-title">Llama.cpp (local)</span>
      <div class="text-muted-foreground grid grid-cols-[8rem_1fr] gap-y-1">
        <span>Runtime</span><span class="text-foreground">llama.cpp on 127.0.0.1 · {{ engine.stateLabel() }}</span>
        <span>This computer</span><span class="text-foreground">{{ models.ramGb() ?? '?' }} GB RAM</span>
        <span>Models</span>
        <span class="text-foreground">
          {{ installedLabel() }} ·
          <a routerLink="/ai/downloads" class="underline">Manage downloads</a>
        </span>
      </div>
      <p class="text-muted-foreground">Runs models on this computer. Nothing you match or research leaves it.</p>
    </div>
  `,
})
export class LocalProviderComponent implements OnInit {
  protected readonly models = inject(ModelsStore);
  protected readonly engine = inject(EngineStore);
  protected readonly installedLabel = computed(() => {
    const n = this.models.installed().length;
    return n === 1 ? '1 model installed' : `${n} models installed`;
  });

  ngOnInit(): void {
    void this.models.load();
    void this.engine.refresh();
  }
}
