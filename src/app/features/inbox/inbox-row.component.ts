import { ChangeDetectionStrategy, Component, computed, inject, input, output } from '@angular/core';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucideChevronDown, lucideExternalLink } from '@ng-icons/lucide';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardTooltipImports } from '@/shared/components/tooltip/tooltip.imports';
import type { JobStatus, ProcessedEmail } from '../../core/ipc/types';
import { JobStatusesStore } from '../../core/stores/job-statuses.store';
import { jobLabel, JobsStore } from '../../core/stores/jobs.store';
import { openJobUrl } from '../jobs/job-url';
import { StatusPillComponent } from '../jobs/status-pill.component';

/** Shown when Undo would overwrite a status set by hand (matches the backend). */
export const UNDO_BLOCKED = 'The status was changed after this update.';

/** One email result in the inbox. */
@Component({
  selector: 'app-inbox-row',
  imports: [ZardButtonComponent, NgIcon, StatusPillComponent, ...ZardTooltipImports],
  viewProviders: [provideIcons({ lucideChevronDown, lucideExternalLink })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    'data-testid': 'inbox-row',
    '[attr.data-message-id]': 'row().message_id',
    class: 'block',
  },
  template: `
    @let r = row();
    <div class="flex items-start gap-3 p-3" [class.opacity-60]="r.outcome === 'dismissed'">
      <span class="mt-1.5 size-2 shrink-0 rounded-full" [class.bg-primary]="unread()" aria-hidden="true">
        @if (unread()) {
          <span data-testid="unread-dot" class="sr-only">Unread</span>
        }
      </span>
      <button
        type="button"
        data-testid="row-toggle"
        class="min-w-0 flex-1 text-left"
        [attr.aria-expanded]="expanded()"
        (click)="toggle.emit()"
      >
        <div class="flex items-baseline gap-2">
          <span class="truncate" [class.font-semibold]="unread()" [class.font-medium]="!unread()">{{ jobName() }}</span>
          <span class="text-muted-foreground shrink-0 text-xs">{{ when() }}</span>
        </div>
        <p class="text-muted-foreground">{{ r.summary }}</p>
        <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
          @switch (variant()) {
            @case ('review') {
              @if (statusOf(r.suggested_status_id); as s) {
                <app-status-pill [name]="s.name" [color]="s.color" />
              }
              <span class="text-muted-foreground tabular-nums">{{ percent() }}%</span>
            }
            @case ('no_change') {
              @if (statusOf(r.previous_status_id); as s) {
                <app-status-pill [name]="s.name" [color]="s.color" />
              }
              <span class="text-muted-foreground">No change</span>
            }
            @case ('dismissed') {
              @if (statusOf(r.suggested_status_id); as s) {
                <app-status-pill [name]="s.name" [color]="s.color" />
              }
              <span class="text-muted-foreground">Dismissed</span>
            }
            @default {
              <span class="contents" [class.line-through]="variant() === 'undone'">
                @if (statusOf(r.previous_status_id); as s) {
                  <app-status-pill [name]="s.name" [color]="s.color" />
                }
                <span aria-hidden="true" class="text-muted-foreground">→</span>
                @if (statusOf(r.suggested_status_id); as s) {
                  <app-status-pill [name]="s.name" [color]="s.color" />
                }
              </span>
              @if (variant() === 'undone') {
                <span class="text-muted-foreground">Undone</span>
              }
            }
          }
        </div>
      </button>
      <div class="flex shrink-0 items-center gap-1.5">
        @switch (variant()) {
          @case ('review') {
            <button z-button zSize="sm" [zDisabled]="busy()" (click)="apply.emit()">Apply</button>
            <button z-button zType="outline" zSize="sm" [zDisabled]="busy()" (click)="dismiss.emit()">Dismiss</button>
          }
          @case ('applied') {
            @if (canUndo()) {
              <button z-button zType="outline" zSize="sm" [zDisabled]="busy()" (click)="undo.emit()">Undo</button>
            } @else {
              <span data-testid="undo-blocked" [title]="undoBlocked" [zTooltip]="undoBlocked">
                <button z-button zType="outline" zSize="sm" [zDisabled]="true">Undo</button>
              </span>
            }
          }
        }
        <button
          type="button"
          class="text-muted-foreground hover:text-foreground p-1"
          aria-label="Open original email in Gmail"
          (click)="openUrl(r.gmail_url)"
        >
          <ng-icon name="lucideExternalLink" />
        </button>
      </div>
    </div>
    @if (expanded()) {
      <div class="px-8 pb-3">
        <div class="text-muted-foreground mb-1 text-xs">{{ r.sender }} · {{ r.subject }}</div>
        @if (r.body) {
          <pre class="bg-muted/40 max-h-64 overflow-y-auto rounded-md p-3 font-sans text-sm whitespace-pre-wrap">{{ r.body }}</pre>
        } @else {
          <p class="text-muted-foreground text-sm">No preview saved for this email.</p>
        }
      </div>
    }
  `,
})
export class InboxRowComponent {
  readonly row = input.required<ProcessedEmail>();
  readonly expanded = input(false);
  readonly busy = input(false);
  readonly toggle = output<void>();
  readonly apply = output<void>();
  readonly dismiss = output<void>();
  readonly undo = output<void>();

  private readonly jobs = inject(JobsStore);
  private readonly statuses = inject(JobStatusesStore);
  protected readonly openUrl = openJobUrl;
  protected readonly undoBlocked = UNDO_BLOCKED;

  protected readonly unread = computed(() => this.row().read_at === null);

  protected readonly variant = computed(() => {
    const r = this.row();
    if (r.outcome === 'pending_review') return 'review';
    if (r.outcome === 'no_change') return 'no_change';
    if (r.outcome === 'dismissed') return 'dismissed';
    return r.undone_at ? 'undone' : 'applied';
  });

  private readonly job = computed(() => this.jobs.jobs().find((j) => j.id === this.row().matched_job_id) ?? null);

  protected readonly jobName = computed(() => {
    const job = this.job();
    return job ? jobLabel(job) : 'Job removed';
  });

  /** Undo is safe only while the job still has the status the email set. */
  protected readonly canUndo = computed(() => {
    const r = this.row();
    return !r.undone_at && this.job()?.status_id === r.suggested_status_id;
  });

  protected readonly percent = computed(() => Math.round((this.row().confidence ?? 0) * 100));

  protected readonly when = computed(() => relativeTime(this.row().received_at));

  protected statusOf(id: number | null): JobStatus | null {
    return id === null ? null : (this.statuses.byId().get(id) ?? null);
  }
}

/** "just now", "5 min ago", "3 h ago", then a short date. */
export function relativeTime(value: string | null, now = Date.now()): string {
  if (!value) return '';
  const time = Date.parse(value);
  if (Number.isNaN(time)) return '';
  const minutes = Math.floor((now - time) / 60_000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  return new Date(time).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}
