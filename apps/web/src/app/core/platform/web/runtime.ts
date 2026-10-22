import type { Transport } from '../transport';
import type { Platform } from '../platform';

/** Replaced by the HTTP/event transport in #65. */
export const transport: Transport = {
  async call(cmd) { throw new Error(`Web transport is not connected: ${cmd}`); },
  async listen() { return () => {}; },
};
export const platform: Platform = {
  capabilities: { autoApply: false, updater: false, revealInFolder: false, localServer: false },
  async openUrl(url) { window.open(url, '_blank', 'noopener,noreferrer'); },
  async saveFile(bytes, suggestedName) {
    const url = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: 'application/pdf' }));
    const link = document.createElement('a');
    link.href = url;
    link.download = suggestedName;
    document.body.append(link);
    link.click();
    link.remove();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
    return suggestedName;
  },
  async reveal() { throw new Error('Revealing local files is unavailable on web'); },
  async openFile() { throw new Error('Opening local files is unavailable on web'); },
  async version() { return 'web'; },
  async checkForUpdates() { return null; },
  async relaunch() { window.location.reload(); },
  async exportCv() { throw new Error('Web PDF export requires the web transport (#65)'); },
};
