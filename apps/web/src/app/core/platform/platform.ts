import type { Selection } from '../ipc/types';
export interface PlatformUpdate {
  version: string;
  downloadAndInstall(): Promise<void>;
}
export interface Platform {
  readonly capabilities: { autoApply: boolean; updater: boolean; revealInFolder: boolean; localServer: boolean };
  openUrl(url: string): Promise<void>;
  saveFile(bytes: Uint8Array, suggestedName: string): Promise<string | null>;
  reveal(path: string): Promise<void>;
  openFile(path: string): Promise<void>;
  version(): Promise<string>;
  checkForUpdates(): Promise<PlatformUpdate | null>;
  relaunch(): Promise<void>;
  exportCv(runId: number, selection: Selection, suggestedName: string): Promise<string | null>;
}
