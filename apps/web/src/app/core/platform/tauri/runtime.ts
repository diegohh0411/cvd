import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getVersion } from '@tauri-apps/api/app';
import { save } from '@tauri-apps/plugin-dialog';
import { openUrl, openPath, revealItemInDir } from '@tauri-apps/plugin-opener';
import { relaunch } from '@tauri-apps/plugin-process';
import { check } from '@tauri-apps/plugin-updater';
import type { Transport } from '../transport';
import type { Platform } from '../platform';

export const transport: Transport = { call: invoke, listen };
export const platform: Platform = {
  capabilities: { autoApply: true, updater: true, revealInFolder: true, localServer: true },
  openUrl, reveal: revealItemInDir,
  openFile: (path) => openPath(path).catch(() => revealItemInDir(path)),
  version: getVersion, checkForUpdates: check, relaunch,
  async saveFile(bytes, suggestedName) {
    return transport.call<string | null>('save_file_bytes', { bytes: Array.from(bytes), suggestedName });
  },
  async exportCv(runId, selection, suggestedName) {
    const picked = await save({ defaultPath: suggestedName, filters: [{ name: 'PDF', extensions: ['pdf'] }] });
    if (!picked) return null;
    const path = /\.pdf$/i.test(picked) ? picked : `${picked}.pdf`;
    await transport.call('export_cv', { runId, selection, path });
    return path;
  },
};
