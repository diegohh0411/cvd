import { ChangeDetectionStrategy, Component, computed, inject, input, OnInit } from '@angular/core';
import { Router } from '@angular/router';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { messageFor } from '../../core/ipc/errors';
import { EngineStore } from '../../core/stores/engine.store';
import { ModelsStore } from '../../core/stores/models.store';
import { PageHeaderComponent } from '../../shell/page-header.component';
import { ModelCardsComponent } from './model-cards.component';
import { RemoteModelsComponent } from './remote-models.component';

type Tab = 'installed' | 'download' | 'online' | 'engine';
const TABS: { id: Tab; label: string }[] = [
  { id: 'installed', label: 'Installed' },
  { id: 'download', label: 'Download' },
  { id: 'online', label: 'Online' },
  { id: 'engine', label: 'Engine' },
];

@Component({
  selector: 'app-models-page',
  imports: [PageHeaderComponent, ModelCardsComponent, RemoteModelsComponent, ZardButtonComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'Models' }, { label: current().label }]" />
    <div class="mx-auto flex w-full max-w-3xl flex-1 flex-col gap-4 p-4">
      <div role="tablist" aria-label="Models" class="bg-muted text-muted-foreground inline-flex w-fit rounded-lg p-1">
        @for (t of tabs; track t.id) {
          <button
            role="tab"
            type="button"
            class="rounded-md px-3 py-1 text-sm font-medium"
            [class]="tabId() === t.id ? 'bg-background text-foreground shadow-sm' : 'hover:text-foreground'"
            [attr.aria-selected]="tabId() === t.id"
            (click)="go(t.id)"
          >
            {{ t.label }}
          </button>
        }
      </div>

      @switch (tabId()) {
        @case ('installed') {
          @if (models.installed().length) {
            <app-model-cards [installedOnly]="true" />
          } @else {
            <div class="text-muted-foreground rounded-xl border border-dashed p-10 text-center text-sm">
              No models installed.
              <button z-button zType="link" (click)="go('download')">Download one</button>
            </div>
          }
        }
        @case ('download') {
          <p class="text-muted-foreground text-sm">
            Models run entirely on this computer; downloading is the only time the app goes online.
            @if (models.ramGb(); as ram) {
              This computer has {{ ram }} GB of RAM.
            }
          </p>
          <app-model-cards />
        }
        @case ('online') {
          <app-remote-models />
        }
        @case ('engine') {
          <div class="grid gap-3 rounded-lg border p-4 text-sm">
            <div class="grid grid-cols-[8rem_1fr] gap-y-2">
              <span class="text-muted-foreground">Model</span><span>{{ engine.modelLabel() }}</span>
              <span class="text-muted-foreground">Status</span><span>{{ engine.stateLabel() }}</span>
              <span class="text-muted-foreground">Runtime</span>
              <span>{{ engine.remote() ? 'OpenRouter (online)' : 'llama.cpp server on 127.0.0.1, started on the first match' }}</span>
            </div>
            @if (engine.status().error; as log) {
              <pre class="bg-muted max-h-60 overflow-auto rounded p-2 text-xs" role="alert">{{ log }}</pre>
            }
            <div>
              <button z-button zType="outline" zSize="sm" [zDisabled]="!engine.status().model_id" (click)="restart()">Restart engine</button>
            </div>
          </div>
        }
      }
    </div>
  `,
})
export class ModelsPageComponent implements OnInit {
  /** `?tab=` query param. */
  readonly tab = input<string | undefined>();
  protected readonly models = inject(ModelsStore);
  protected readonly engine = inject(EngineStore);
  private readonly router = inject(Router);
  private readonly toast = inject(ZardSonnerService);
  protected readonly tabs = TABS;
  protected readonly tabId = computed<Tab>(() => (TABS.some((t) => t.id === this.tab()) ? (this.tab() as Tab) : 'installed'));
  protected readonly current = computed(() => TABS.find((t) => t.id === this.tabId())!);

  ngOnInit(): void {
    void this.models.load();
    void this.engine.refresh();
  }

  go(tab: Tab): void {
    void this.router.navigate([], { queryParams: { tab } });
  }

  /** Re-selecting the active model resets the engine (and its error). */
  async restart(): Promise<void> {
    const id = this.engine.status().model_id;
    if (!id) return;
    try {
      await this.models.activate(id);
      await this.engine.refresh();
      this.toast.success('Engine restarted', { description: 'It starts again on the next match.' });
    } catch (e) {
      this.toast.error('Could not restart', { description: messageFor(e) });
      if (this.engine.status().state === 'missing') this.go('download');
    }
  }
}
