import { computed, Injectable, signal } from '@angular/core';

import * as api from '../ipc/commands';
import { messageFor } from '../ipc/errors';
import type { Profile } from '../ipc/types';

export const EMPTY_PROFILE: Profile = { name: '', headline: '', email: '', phone: '', location: '', links: [], summary: '' };

@Injectable({ providedIn: 'root' })
export class ProfileStore {
  readonly profile = signal<Profile>(EMPTY_PROFILE);
  readonly loaded = signal(false);
  readonly error = signal<string | null>(null);
  /** First run: no profile saved yet. */
  readonly isEmpty = computed(() => this.loaded() && !this.profile().name.trim());

  async load(): Promise<void> {
    try {
      this.profile.set(await api.getProfile());
      this.error.set(null);
    } catch (e) {
      this.error.set(messageFor(e));
    } finally {
      this.loaded.set(true);
    }
  }

  async save(profile: Profile): Promise<void> {
    await api.saveProfile(profile);
    this.profile.set(profile);
  }
}
