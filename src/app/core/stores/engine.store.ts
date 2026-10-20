import { computed, Injectable, signal } from '@angular/core';

import * as api from '../ipc/commands';
import { onEngineStatus } from '../ipc/events';
import type { EngineStatus } from '../ipc/types';

/** Model references include their connection; bare ids belong to legacy history. */
const REMOTE_PREFIX = /^[a-z][a-z0-9~-]*:/;

const LABELS: Record<EngineStatus['state'], string> = {
  none: 'no model',
  missing: 'not installed',
  idle: 'ready',
  ready: 'ready',
  error: 'error',
};

@Injectable({ providedIn: 'root' })
export class EngineStore {
  readonly status = signal<EngineStatus>({ model_id: null, state: 'none', error: null });
  private listening = false;

  readonly modelLabel = computed(() => {
    const id = this.status().model_id;
    if (!id) return 'No model';
    const remote = REMOTE_PREFIX.exec(id);
    if (remote) return `${id.slice(remote[0].length)} (${id.startsWith('localhost:') ? 'local' : 'online'})`;
    return id;
  });
  readonly remote = computed(() => REMOTE_PREFIX.test(this.status().model_id ?? '') && !this.status().model_id?.startsWith('localhost:'));
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
