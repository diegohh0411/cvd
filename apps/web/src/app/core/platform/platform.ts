import type { Selection } from '../ipc/types';
export interface PlatformUpdate {
  version: string;
  downloadAndInstall(): Promise<void>;
}
/** Hosted accounts; desktop and the dev server have none. */
export interface PlatformAuth {
  /** Restores any session (including one returned in the URL); `null` when there are no accounts. */
  init(): Promise<{ signedIn: boolean; email: string | null } | null>;
  signInWithGoogle(): Promise<void>;
  /** Sends a magic link; it signs up unknown addresses. */
  signInWithEmail(email: string): Promise<void>;
  signOut(): Promise<void>;
}
export interface Platform {
  readonly auth: PlatformAuth;
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
