import { computed, inject, Injectable, signal } from '@angular/core';

import * as api from '../ipc/commands';
import { onEngineStatus } from '../ipc/events';
import type { EngineStatus } from '../ipc/types';
import { ModelsStore } from './models.store';

const REMOTE_PREFIX = 'openrouter:';

const LABELS: Record<EngineStatus['state'], string> = {
  none: 'no model',
  missing: 'not installed',
  idle: 'ready',
  ready: 'running',
  error: 'error',
};

@Injectable({ providedIn: 'root' })
export class EngineStore {
  private readonly models = inject(ModelsStore);
  readonly status = signal<EngineStatus>({ model_id: null, state: 'none', error: null });
  private listening = false;

  readonly modelLabel = computed(() => {
    const id = this.status().model_id;
    if (!id) return 'No model';
    if (id.startsWith(REMOTE_PREFIX)) return `${id.slice(REMOTE_PREFIX.length)} (online)`;
    const spec = this.models.models().find((m) => m.spec.id === id)?.spec;
    return spec ? spec.label.replace(/\s*\(.*\)$/, '') : id;
  });
  readonly remote = computed(() => this.status().model_id?.startsWith(REMOTE_PREFIX) ?? false);
  readonly stateLabel = computed(() => LABELS[this.status().state]);
  readonly ok = computed(() => this.status().state === 'idle' || this.status().state === 'ready');

  async load(): Promise<void> {
    if (!this.listening) {
      this.listening = true;
      void onEngineStatus(() => void this.refresh());
    }
    await this.refresh();
  }

  async refresh(): Promise<void> {
    try {
      this.status.set(await api.engineStatus());
    } catch {
      // Keep the last known status; the footer is informational.
    }
  }
}
