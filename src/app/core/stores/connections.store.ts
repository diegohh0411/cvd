import { computed, Injectable, signal } from '@angular/core';

import * as api from '../ipc/commands';
import { onTablesChanged } from '../ipc/events';
import type { Connection, ProviderId } from '../ipc/types';

/** Saved model-provider keys, shared by the model pickers and the Connections list. */
@Injectable({ providedIn: 'root' })
export class ConnectionsStore {
  readonly revision = signal(0);
  readonly list = signal<Connection[]>([]);
  /** Connection ids, for "does this model's key exist". */
  readonly ids = computed(() => new Set(this.list().map((c) => c.id)));

  private loadedOnce = false;

  constructor() {
    void onTablesChanged(['secrets'], () => {
      if (this.loadedOnce) void this.load();
    }).catch(() => undefined);
  }

  async load(): Promise<void> {
    this.loadedOnce = true;
    try {
      this.list.set(await api.listConnections());
      this.revision.update(n => n + 1);
    } catch {
      // Keep the last known list; the pickers show a locked group instead.
    }
  }

  forProvider(provider: ProviderId): Connection[] {
    return this.list().filter((c) => c.provider === provider);
  }

  async add(provider: ProviderId, label: string, key: string): Promise<void> {
    await api.addConnection(provider, label, key);
    await this.load();
  }

  async remove(id: string): Promise<void> {
    await api.deleteProviderKey(id as ProviderId);
    await this.load();
  }

  async rename(id: string, label: string): Promise<void> {
    await api.renameConnection(id, label);
    await this.load();
  }
}
