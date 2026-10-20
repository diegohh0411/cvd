import { ChangeDetectionStrategy, Component, computed, inject, OnInit, signal } from '@angular/core';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { InboxFilter, ProcessedEmail } from '../../core/ipc/types';
import { InboxStore } from '../../core/stores/inbox.store';
import { JobStatusesStore } from '../../core/stores/job-statuses.store';
import { JobsStore } from '../../core/stores/jobs.store';
import { ScanStore } from '../../core/stores/scan.store';
import { PageHeaderComponent } from '../../shell/page-header.component';
import { InboxRowComponent, relativeTime } from './inbox-row.component';

const TABS: readonly { filter: InboxFilter; label: string }[] = [
  { filter: 'all', label: 'All' },
  { filter: 'review', label: 'Needs review' },
  { filter: 'unread', label: 'Unread' },
];

const MAIL_ERRORS = new Set(['MailAuth', 'MailUnavailable']);

/** Email results from the Gmail watcher: notifications and decisions to review. */
@Component({
  selector: 'app-inbox-page',
  imports: [PageHeaderComponent, ZardButtonComponent, RouterLink, InboxRowComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'Inbox' }]">
      <button z-button zType="outline" zSize="sm" [zDisabled]="!inbox.unread()" (click)="markAllRead()">Mark all read</button>
      <button z-button zSize="sm" [zLoading]="scan.running()" [zDisabled]="checkDisabled()" (click)="checkNow()">Check now</button>
    </app-page-header>
    <div class="flex flex-1 flex-col gap-4 p-4">
      <div data-testid="watcher-status" class="text-muted-foreground flex flex-wrap items-center gap-2 text-sm" role="status">
        @switch (status().kind) {
          @case ('paused') {
            <span class="text-destructive">Paused: {{ status().text }}</span>
            <a class="underline" [routerLink]="status().link">{{ status().linkLabel }}</a>
            <button z-button zType="outline" zSize="sm" (click)="retry()">Retry</button>
          }
          @case ('disconnected') {
            <a class="underline" routerLink="/settings">{{ status().text }}</a>
          }
          @default {
            <span>{{ status().text }}</span>
          }
        }
      </div>

      <div role="tablist" aria-label="Filter the inbox" class="bg-muted inline-flex w-fit rounded-lg p-1">
        @for (tab of tabs; track tab.filter) {
          <button
            type="button"
            role="tab"
            class="rounded-md px-3 py-1 text-sm"
            [class.bg-background]="inbox.filter() === tab.filter"
            [class.shadow-sm]="inbox.filter() === tab.filter"
            [attr.aria-selected]="inbox.filter() === tab.filter"
            (click)="selectTab(tab.filter)"
          >{{ tab.label }}</button>
        }
      </div>

      @if (inbox.items().length) {
        <ul class="divide-y rounded-md border text-sm">
          @for (r of inbox.items(); track r.message_id) {
            <li>
              <app-inbox-row
                [row]="r"
                [expanded]="expanded() === r.message_id"
                [busy]="busy()"
                (toggle)="toggle(r)"
                (apply)="act(r, 'apply')"
                (dismiss)="act(r, 'dismiss')"
                (undo)="act(r, 'undo')"
              />
            </li>
          }
        </ul>
      } @else {
        <p class="text-muted-foreground py-10 text-center text-sm">{{ emptyText() }}</p>
      }
    </div>
  `,
})
export class InboxPageComponent implements OnInit {
  protected readonly inbox = inject(InboxStore);
  protected readonly scan = inject(ScanStore);
  private readonly jobs = inject(JobsStore);
  private readonly statuses = inject(JobStatusesStore);
  private readonly route = inject(ActivatedRoute);
  private readonly router = inject(Router);
  private readonly toast = inject(ZardSonnerService);
  protected readonly tabs = TABS;

  protected readonly expanded = signal<string | null>(null);
  protected readonly busy = signal(false);
  private readonly mailConnected = signal<boolean | null>(null);

  protected readonly checkDisabled = computed(() => this.scan.running() || this.mailConnected() !== true);

  protected readonly status = computed(() => {
    const w = this.inbox.watcher();
    if (this.mailConnected() === false || !w || w.state === 'stopped') {
      return { kind: 'disconnected', text: 'Connect Gmail in Settings › Integrations.' } as const;
    }
    if (w.state === 'paused') {
      const mail = MAIL_ERRORS.has(w.error_kind ?? '');
      return {
        kind: 'paused',
        text: w.error ?? 'Unknown error',
        link: mail ? '/settings' : '/ai',
        linkLabel: mail ? 'Settings › Integrations' : 'AI › AI Models',
      } as const;
    }
    if (w.state === 'scanning' || this.scan.running()) {
      const p = this.scan.progress();
      return { kind: 'info', text: p ? `Checking ${p.checked} / ${p.total} emails…` : 'Checking emails…' } as const;
    }
    if (w.state === 'connecting') return { kind: 'info', text: 'Connecting…' } as const;
    const checked = relativeTime(w.last_checked_at);
    return { kind: 'info', text: checked ? `Watching Gmail · checked ${checked}` : 'Watching Gmail' } as const;
  });

  protected readonly emptyText = computed(() => {
    switch (this.inbox.filter()) {
      case 'review':
        return 'Nothing to review.';
      case 'unread':
        return 'All caught up.';
      default:
        return 'No job updates yet. New emails about your applications show up here.';
    }
  });

  ngOnInit(): void {
    void this.jobs.load();
    void this.statuses.ensureLoaded();
    void this.scan.ensureReady();
    void api.getMailAccount().then(
      (a) => this.mailConnected.set(a !== null),
      () => this.mailConnected.set(false),
    );
    const param = this.route.snapshot.queryParamMap.get('filter');
    const filter = TABS.find((t) => t.filter === param)?.filter ?? 'all';
    void this.inbox.ensureReady().then(() => this.inbox.setFilter(filter));
  }

  protected selectTab(filter: InboxFilter): void {
    void this.router.navigate(['/inbox'], { queryParams: { filter: filter === 'all' ? null : filter } });
    void this.inbox.setFilter(filter);
  }

  protected toggle(r: ProcessedEmail): void {
    const opening = this.expanded() !== r.message_id;
    this.expanded.set(opening ? r.message_id : null);
    if (opening) void this.inbox.markRead(r.message_id).catch(() => undefined);
  }

  protected async act(r: ProcessedEmail, action: 'apply' | 'dismiss' | 'undo'): Promise<void> {
    this.busy.set(true);
    try {
      if (action === 'apply') await this.inbox.apply(r.message_id);
      else if (action === 'dismiss') await this.inbox.dismiss(r.message_id);
      else await this.inbox.undo(r.message_id);
      if (action !== 'dismiss') void this.jobs.load();
      void this.scan.refreshPending();
    } catch (e) {
      this.toast.error('Could not update this job', { description: messageFor(e) });
    } finally {
      this.busy.set(false);
    }
  }

  protected async markAllRead(): Promise<void> {
    try {
      await this.inbox.markAllRead();
    } catch (e) {
      this.toast.error('Could not mark the inbox read', { description: messageFor(e) });
    }
  }

  protected async checkNow(): Promise<void> {
    try {
      await this.scan.start();
    } catch (e) {
      this.toast.error('Could not check for new emails', { description: messageFor(e) });
    }
  }

  protected async retry(): Promise<void> {
    try {
      await this.inbox.retry();
    } catch (e) {
      this.toast.error('Could not retry', { description: messageFor(e) });
    }
  }
}
