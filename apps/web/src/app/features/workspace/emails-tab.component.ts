import { ChangeDetectionStrategy, Component, DestroyRef, effect, inject, input, signal } from '@angular/core';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucideExternalLink } from '@ng-icons/lucide';

import { formatTimestamp } from '../../core/format';
import * as api from '../../core/ipc/commands';
import { onTablesChanged, untilDestroyed } from '../../core/ipc/events';
import type { JobStatus, ProcessedEmail } from '../../core/ipc/types';
import { JobStatusesStore } from '../../core/stores/job-statuses.store';
import { openJobUrl } from '../jobs/job-url';
import { StatusPillComponent } from '../jobs/status-pill.component';

/** Emails the Gmail scan matched to this job, newest first. */
@Component({
  selector: 'app-emails-tab',
  imports: [NgIcon, StatusPillComponent],
  viewProviders: [provideIcons({ lucideExternalLink })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <ul class="divide-y rounded-lg border" aria-label="Emails for this job">
      @for (e of emails(); track e.message_id) {
        <li class="flex items-start gap-3 px-3 py-2">
          <div class="min-w-0 flex-1 text-sm">
            <div class="truncate font-medium">{{ e.subject || '(no subject)' }}</div>
            <div class="text-muted-foreground truncate text-xs">{{ e.sender }} · {{ date(e.received_at) }}</div>
            @if (e.summary) {
              <p class="text-muted-foreground mt-1">{{ e.summary }}</p>
            }
            @if (statusOf(e.suggested_status_id); as s) {
              <div class="mt-1.5 flex items-center gap-1.5">
                <app-status-pill [name]="s.name" [color]="s.color" />
                @if (e.outcome === 'pending_review') {
                  <span class="text-muted-foreground text-xs">Needs review</span>
                } @else if (e.outcome === 'dismissed') {
                  <span class="text-muted-foreground text-xs">Dismissed</span>
                }
              </div>
            }
          </div>
          <button type="button" class="text-muted-foreground hover:text-foreground p-1" aria-label="Open original email in Gmail" (click)="openUrl(e.gmail_url)">
            <ng-icon name="lucideExternalLink" />
          </button>
        </li>
      } @empty {
        <li class="text-muted-foreground p-6 text-center text-sm">No emails for this job yet.</li>
      }
    </ul>
  `,
})
export class EmailsTabComponent {
  readonly jobId = input.required<number>();
  protected readonly emails = signal<ProcessedEmail[]>([]);
  protected readonly date = formatTimestamp;
  protected readonly openUrl = openJobUrl;
  private readonly statuses = inject(JobStatusesStore);

  /** Bumped when the table changes elsewhere. */
  private readonly changes = signal(0);

  constructor() {
    void untilDestroyed(inject(DestroyRef), onTablesChanged(['processed_email'], () => this.changes.update((n) => n + 1))).catch(() => undefined);
    effect(() => {
      this.changes();
      const id = this.jobId();
      void api.listJobEmails(id).then((r) => this.emails.set(r), () => this.emails.set([]));
    });
  }

  protected statusOf(id: number | null): JobStatus | null {
    return id === null ? null : (this.statuses.byId().get(id) ?? null);
  }
}
