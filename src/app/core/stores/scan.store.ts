import { Injectable, signal } from '@angular/core';

import * as api from '../ipc/commands';
import { onScanFinished, onScanProgress } from '../ipc/events';
import type { AppErrorKind, ScanFinished, ScanProgress } from '../ipc/types';

/** The counts of a finished scan, as shown in the results popup header. */
export type ScanSummary = Omit<ScanFinished, 'status' | 'error' | 'error_kind'>;

/**
 * The Gmail scan, shared app-wide. Events are subscribed once, so a scan that
 * finishes while the Jobs page is closed still updates the badge and popup.
 */
@Injectable({ providedIn: 'root' })
export class ScanStore {
  readonly running = signal(false);
  readonly progress = signal<ScanProgress | null>(null);
  /** Set when a scan finishes; the Jobs page opens the popup and acknowledges it. */
  readonly finishedRunId = signal<number | null>(null);
  readonly lastError = signal<string | null>(null);
  readonly lastErrorKind = signal<AppErrorKind | null>(null);
  readonly finishedSummary = signal<ScanSummary | null>(null);
  readonly pendingCount = signal(0);

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
    try {
      await api.startScan();
    } catch (e) {
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

  acknowledgeFinished(): void {
    this.finishedRunId.set(null);
  }

  private async setUp(): Promise<void> {
    await Promise.all([
      onScanProgress((p) => this.progress.set(p)),
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
    this.running.set(false);
    this.progress.set(null);
    this.finishedRunId.set(f.run_id);
    this.lastError.set(f.error);
    this.lastErrorKind.set(f.error_kind);
    this.finishedSummary.set({
      run_id: f.run_id,
      checked: f.checked,
      job_related: f.job_related,
      applied: f.applied,
      pending: f.pending,
      no_change: f.no_change,
      failed: f.failed,
    });
    void this.refreshPending();
  }
}
