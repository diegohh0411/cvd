import { ChangeDetectionStrategy, Component, computed, effect, inject, input, model, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucideExternalLink } from '@ng-icons/lucide';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardDialogImports } from '@/shared/components/dialog/dialog.imports';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { AppErrorKind, JobStatus, ProcessedEmail } from '../../core/ipc/types';
import { JobStatusesStore } from '../../core/stores/job-statuses.store';
import { jobLabel, JobsStore } from '../../core/stores/jobs.store';
import { ScanStore } from '../../core/stores/scan.store';
import { openJobUrl } from './job-url';
import { StatusPillComponent } from './status-pill.component';

/**
 * Results of one scan (`runId` set, opened right after it finishes) or the
 * review queue (`runId` null, opened from the badge).
 */
@Component({
  selector: 'app-scan-results-dialog',
  imports: [...ZardDialogImports, ZardButtonComponent, NgIcon, RouterLink, StatusPillComponent],
  viewProviders: [provideIcons({ lucideExternalLink })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <z-dialog [zVisible]="open()" (zVisibleChange)="open.set($event)" class="sm:max-w-2xl">
      <z-dialog-header>
        <z-dialog-title>Job updates from Gmail</z-dialog-title>
        <z-dialog-description>{{ header() }}</z-dialog-description>
      </z-dialog-header>

      <div class="max-h-[60vh] space-y-5 overflow-y-auto py-2 text-sm" data-testid="scan-results-body">
        @if (failure(); as f) {
          <div role="alert" class="rounded-md border border-red-300 bg-red-50 p-3 text-red-900 dark:border-red-800 dark:bg-red-950/40 dark:text-red-100">
            <p class="font-medium">The scan stopped: {{ f.message }}</p>
            @if (f.kind === 'MailAuth' || f.kind === 'MailUnavailable') {
              <p>Check your connection in <a routerLink="/settings" class="underline" (click)="open.set(false)">Settings › Integrations</a>.</p>
            } @else if (f.kind === 'ProviderAuth' || f.kind === 'ProviderCredits' || f.kind === 'ProviderUnavailable') {
              <p>Check your AI key in <a routerLink="/ai" class="underline" (click)="open.set(false)">AI › AI Models</a>.</p>
            }
          </div>
        }

        @if (applied().length) {
          <section aria-labelledby="scan-applied">
            <h3 id="scan-applied" class="mb-1.5 font-medium">Applied <span class="text-muted-foreground tabular-nums">{{ applied().length }}</span></h3>
            <ul class="divide-y rounded-md border">
              @for (r of applied(); track r.message_id) {
                <li class="flex items-start gap-3 p-3">
                  <div class="min-w-0 flex-1">
                    <div class="truncate font-medium">{{ jobName(r) }}</div>
                    <p class="text-muted-foreground">{{ r.summary }}</p>
                    <div class="mt-1.5 flex items-center gap-1.5">
                      @if (statusOf(r.previous_status_id); as s) {
                        <app-status-pill [name]="s.name" [color]="s.color" />
                      }
                      <span aria-hidden="true" class="text-muted-foreground">→</span>
                      @if (statusOf(r.suggested_status_id); as s) {
                        <app-status-pill [name]="s.name" [color]="s.color" />
                      }
                    </div>
                  </div>
                  <button type="button" class="text-muted-foreground hover:text-foreground p-1" aria-label="Open original email in Gmail" (click)="openUrl(r.gmail_url)">
                    <ng-icon name="lucideExternalLink" />
                  </button>
                </li>
              }
            </ul>
          </section>
        }

        @if (review().length) {
          <section aria-labelledby="scan-review">
            <h3 id="scan-review" class="mb-1.5 font-medium">Needs review <span class="text-muted-foreground tabular-nums">{{ review().length }}</span></h3>
            <ul class="divide-y rounded-md border">
              @for (r of review(); track r.message_id) {
                <li class="flex items-start gap-3 p-3">
                  <div class="min-w-0 flex-1">
                    <div class="truncate font-medium">{{ jobName(r) }}</div>
                    <p class="text-muted-foreground">{{ r.summary }}</p>
                    <div class="mt-1.5 flex flex-wrap items-center gap-2">
                      @if (statusOf(r.suggested_status_id); as s) {
                        <app-status-pill [name]="s.name" [color]="s.color" />
                      }
                      <span class="text-muted-foreground tabular-nums">{{ percent(r) }}% confident</span>
                    </div>
                  </div>
                  <div class="flex shrink-0 items-center gap-1.5">
                    <button z-button zSize="sm" [zDisabled]="busy()" (click)="apply(r)">Apply</button>
                    <button z-button zType="outline" zSize="sm" [zDisabled]="busy()" (click)="dismiss(r)">Dismiss</button>
                    <button type="button" class="text-muted-foreground hover:text-foreground p-1" aria-label="Open original email in Gmail" (click)="openUrl(r.gmail_url)">
                      <ng-icon name="lucideExternalLink" />
                    </button>
                  </div>
                </li>
              }
            </ul>
          </section>
        }

        @if (noChange().length) {
          <section aria-labelledby="scan-no-change">
            <h3 id="scan-no-change" class="mb-1.5 font-medium">No change <span class="text-muted-foreground tabular-nums">{{ noChange().length }}</span></h3>
            <ul class="divide-y rounded-md border">
              @for (r of noChange(); track r.message_id) {
                <li class="flex items-start gap-3 p-3">
                  <div class="min-w-0 flex-1">
                    <div class="truncate font-medium">{{ jobName(r) }}</div>
                    <p class="text-muted-foreground">{{ r.summary }}</p>
                    <div class="mt-1.5 flex items-center gap-2">
                      @if (statusOf(r.previous_status_id); as s) {
                        <app-status-pill [name]="s.name" [color]="s.color" />
                      }
                      <span class="text-muted-foreground">No change</span>
                    </div>
                  </div>
                  <button type="button" class="text-muted-foreground hover:text-foreground p-1" aria-label="Open original email in Gmail" (click)="openUrl(r.gmail_url)">
                    <ng-icon name="lucideExternalLink" />
                  </button>
                </li>
              }
            </ul>
          </section>
        }

        @if (!applied().length && !review().length && !noChange().length && !failure()) {
          <p class="text-muted-foreground py-6 text-center">No job updates found in the last 30 days.</p>
        }
      </div>
    </z-dialog>
  `,
})
export class ScanResultsDialogComponent {
  /** The scan run to list; `null` shows only the review queue. */
  readonly runId = input<number | null>(null);
  readonly open = model(false);

  protected readonly scan = inject(ScanStore);
  protected readonly jobs = inject(JobsStore);
  protected readonly statuses = inject(JobStatusesStore);
  private readonly toast = inject(ZardSonnerService);
  protected readonly openUrl = openJobUrl;

  private readonly results = signal<ProcessedEmail[]>([]);
  protected readonly review = signal<ProcessedEmail[]>([]);
  protected readonly busy = signal(false);

  protected readonly applied = computed(() => this.results().filter((r) => r.outcome === 'applied'));
  protected readonly noChange = computed(() => this.results().filter((r) => r.outcome === 'no_change'));

  /** Counts come from the finished scan when this dialog shows that scan. */
  protected readonly header = computed(() => {
    const pending = this.review().length;
    const summary = this.scan.finishedSummary();
    const runId = this.runId();
    if (runId !== null && summary?.run_id === runId) {
      return `${summary.checked} emails from the last 30 days · ${summary.job_related} job-related · ${pending} need review · ${summary.no_change} no change`;
    }
    return `${pending} to review`;
  });

  protected readonly failure = computed<{ message: string; kind: AppErrorKind | null } | null>(() => {
    const runId = this.runId();
    if (runId === null || this.scan.finishedSummary()?.run_id !== runId) return null;
    const message = this.scan.lastError();
    return message ? { message, kind: this.scan.lastErrorKind() } : null;
  });

  constructor() {
    effect(() => {
      if (this.open()) void this.load();
    });
  }

  protected jobName(r: ProcessedEmail): string {
    const job = this.jobs.jobs().find((j) => j.id === r.matched_job_id);
    return job ? jobLabel(job) : 'Job removed';
  }

  protected statusOf(id: number | null): JobStatus | null {
    return id === null ? null : (this.statuses.byId().get(id) ?? null);
  }

  protected percent(r: ProcessedEmail): number {
    return Math.round((r.confidence ?? 0) * 100);
  }

  protected async apply(r: ProcessedEmail): Promise<void> {
    await this.act(
      () => api.applySuggestion(r.message_id),
      r.message_id,
      'Status updated',
    );
    void this.jobs.load();
  }

  protected async dismiss(r: ProcessedEmail): Promise<void> {
    await this.act(() => api.dismissSuggestion(r.message_id), r.message_id, 'Suggestion dismissed');
  }

  private async load(): Promise<void> {
    const runId = this.runId();
    try {
      const [run, pending] = await Promise.all([
        runId === null ? Promise.resolve<ProcessedEmail[]>([]) : api.listScanResults(runId),
        api.listPendingReviews(),
      ]);
      this.results.set(run);
      this.review.set(pending);
    } catch (e) {
      this.toast.error('Could not load job updates', { description: messageFor(e) });
    }
  }

  private async act(run: () => Promise<unknown>, messageId: string, success: string): Promise<void> {
    this.busy.set(true);
    try {
      await run();
      this.review.update((list) => list.filter((r) => r.message_id !== messageId));
      this.toast.success(success);
      void this.scan.refreshPending();
    } catch (e) {
      this.toast.error('Could not update this job', { description: messageFor(e) });
    } finally {
      this.busy.set(false);
    }
  }
}
