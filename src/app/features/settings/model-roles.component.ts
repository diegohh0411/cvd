import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { ModelInfo, ProviderModels } from '../../core/ipc/types';
import { EngineStore } from '../../core/stores/engine.store';

export interface RoleOption {
  id: string;
  label: string;
}

const remoteLabel = (id: string) => `${id.slice(id.indexOf(':') + 1)} (OpenRouter)`;

/** Installed local chat models, then online models that support tools. */
export function researchOptions(models: ModelInfo[], remote: ProviderModels[]): RoleOption[] {
  const local = models.filter((m) => m.installed && m.spec.kind === 'chat').map((m) => ({ id: m.spec.id, label: `${m.spec.id} (local)` }));
  const online = remote.flatMap((p) => p.models.map((m) => ({ id: `${p.provider}:${m.id}`, label: `${m.display_name} (OpenRouter)` })));
  return [...local, ...online];
}

/** Installed local decision models, plus the active one when it is online. */
export function systemOneOptions(models: ModelInfo[], active: string | null): RoleOption[] {
  const local = models.filter((m) => m.installed && m.spec.kind === 'decision').map((m) => ({ id: m.spec.id, label: `${m.spec.id} (local)` }));
  return active && active.includes(':') ? [...local, { id: active, label: remoteLabel(active) }] : local;
}

/** Which model serves each role: System One scoring and company research. */
@Component({
  selector: 'app-model-roles',
  imports: [RouterLink],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid gap-4 text-sm">
      <label class="grid max-w-md gap-1.5">
        <span class="font-medium">System One</span>
        <select class="border-input h-8 rounded-md border bg-transparent px-2" aria-label="System One model" [value]="engine.status().model_id ?? ''" (change)="setSystemOne($any($event.target).value)">
          <option value="" disabled>Choose a model</option>
          @for (o of systemOne(); track o.id) {
            <option [value]="o.id">{{ o.label }}</option>
          }
        </select>
        <span class="text-muted-foreground text-xs">Scores your library against each job. More on the <a routerLink="/models" class="underline">Models</a> page.</span>
      </label>
      <label class="grid max-w-md gap-1.5">
        <span class="font-medium">Company research</span>
        <select class="border-input h-8 rounded-md border bg-transparent px-2" aria-label="Company research model" [value]="research() ?? ''" (change)="setResearch($any($event.target).value)">
          <option value="" disabled>Choose a model</option>
          @for (o of researchOpts(); track o.id) {
            <option [value]="o.id">{{ o.label }}</option>
          }
        </select>
        <span class="text-muted-foreground text-xs">
          Researches each company on the web (needs an Exa key). Download a local research model on the
          <a routerLink="/models" class="underline">Models</a> page, or add an OpenRouter key to use an online one.
        </span>
      </label>
    </div>
  `,
})
export class ModelRolesComponent implements OnInit {
  protected readonly engine = inject(EngineStore);
  private readonly toast = inject(ZardSonnerService);
  protected readonly systemOne = signal<RoleOption[]>([]);
  protected readonly researchOpts = signal<RoleOption[]>([]);
  protected readonly research = signal<string | null>(null);

  async ngOnInit(): Promise<void> {
    const [models, research, remote] = await Promise.all([
      api.listModels().catch(() => [] as ModelInfo[]),
      api.getResearchModel().catch(() => null),
      api.listRemoteModels(false, 'chat').catch(() => [] as ProviderModels[]),
    ]);
    this.research.set(research);
    this.researchOpts.set(researchOptions(models, remote));
    this.systemOne.set(systemOneOptions(models, this.engine.status().model_id));
  }

  protected async setSystemOne(id: string): Promise<void> {
    try {
      await api.setActiveModel(id);
      await this.engine.refresh();
    } catch (e) {
      this.toast.error('Could not switch model', { description: messageFor(e) });
    }
  }

  protected async setResearch(id: string): Promise<void> {
    try {
      await api.setResearchModel(id);
      this.research.set(id);
    } catch (e) {
      this.toast.error('Could not set research model', { description: messageFor(e) });
    }
  }
}
