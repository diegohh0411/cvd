import { inject, Injectable, signal } from '@angular/core';

import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../ipc/commands';
import { toAppError } from '../ipc/errors';
import { onScanFinished, onScanProgress } from '../ipc/events';
import type { AppErrorKind, ScanFinished, ScanProgress } from '../ipc/types';

/** What `start_scan` answers while another scan holds the slot. */
const SCAN_BUSY = 'A scan is already running.';

/**
 * The Gmail scan, shared app-wide. Scans run in the background (the mail
 * watcher) or from "Check now"; only the latter announces its result.
 */
@Injectable({ providedIn: 'root' })
export class ScanStore {
  readonly running = signal(false);
  readonly progress = signal<ScanProgress | null>(null);
  readonly lastError = signal<string | null>(null);
  readonly lastErrorKind = signal<AppErrorKind | null>(null);
  readonly pendingCount = signal(0);

  private readonly toast = inject(ZardSonnerService);
  /** Set by `start()`, so only the scan the user asked for is toasted. */
  private manualStarted = false;

  private ready: Promise<void> | null = null;

  /** Subscribes to scan events and reads whether a scan is already running. Runs once. */
  ensureReady(): Promise<void> {
    return (this.ready ??= this.setUp());
  }

  async start(): Promise<void> {
    await this.ensureReady();
    if (this.running()) return;
    this.running.set(true);
    this.progress.set(null);
    this.lastError.set(null);
    this.manualStarted = true;
    try {
      await api.startScan();
    } catch (e) {
      this.manualStarted = false;
      // The watcher holds the slot; its scan is already showing as running.
      if (toAppError(e).message === SCAN_BUSY) return;
      this.running.set(false);
      throw e;
    }
  }

  async refreshPending(): Promise<void> {
    try {
      this.pendingCount.set((await api.listPendingReviews()).length);
    } catch {
      // Keep the last known count; the badge is informational.
    }
  }

  private async setUp(): Promise<void> {
    await Promise.all([
      onScanProgress((p) => {
        // Background scans start without `start()`.
        this.running.set(true);
        this.progress.set(p);
      }),
      onScanFinished((f) => this.finish(f)),
    ]);
    try {
      this.running.set((await api.getScanStatus()).running);
    } catch {
      // Assume idle; a scan started from this window will set it.
    }
    await this.refreshPending();
  }

  private finish(f: ScanFinished): void {
    // A re-check runs first inside the same cycle; the scan still follows.
    if (f.source === 'recheck') return;
    this.running.set(false);
    this.progress.set(null);
    this.lastError.set(f.error);
    this.lastErrorKind.set(f.error_kind);
    if (f.source === 'manual' && this.manualStarted) {
      this.manualStarted = false;
      this.announce(f);
    }
    void this.refreshPending();
  }

  private announce(f: ScanFinished): void {
    if (f.status === 'failed') {
      this.toast.error('The check stopped', { description: f.error ?? undefined });
      return;
    }
    const updates = f.applied + f.pending;
    if (updates === 0) this.toast.info('No new updates');
    else this.toast.success(updates === 1 ? '1 update' : `${updates} updates`);
  }
}
