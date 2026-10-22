import { inject, Injectable } from '@angular/core';
import { relaunch } from '@tauri-apps/plugin-process';
import { check } from '@tauri-apps/plugin-updater';

import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { messageFor } from './ipc/errors';

/** Checks GitHub Releases for a newer signed build and offers to install it. */
@Injectable({ providedIn: 'root' })
export class UpdateService {
  private readonly toast = inject(ZardSonnerService);

  /** Silent on "up to date" and on network errors, so startup never nags offline users. */
  async checkOnStartup(): Promise<void> {
    try {
      const update = await check();
      if (!update) return;
      this.toast.info(`Update available: v${update.version}`, {
        description: 'Download and restart to install.',
        duration: Infinity,
        action: { label: 'Install', onClick: () => void this.install(update) },
      });
    } catch {
      // No network, no release yet, or running in dev without a signed build.
    }
  }

  private async install(update: NonNullable<Awaited<ReturnType<typeof check>>>): Promise<void> {
    const id = this.toast.loading('Downloading update…');
    try {
      await update.downloadAndInstall();
      await relaunch();
    } catch (e) {
      this.toast.error('Update failed', { id, description: messageFor(e) });
    }
  }
}
