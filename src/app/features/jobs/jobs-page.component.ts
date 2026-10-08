import { ChangeDetectionStrategy, Component, computed, inject, OnInit, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucidePlus } from '@ng-icons/lucide';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSkeletonComponent } from '@/shared/components/skeleton/skeleton.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { formatTimestamp } from '../../core/format';
import { messageFor } from '../../core/ipc/errors';
import type { JobDescription } from '../../core/ipc/types';
import { JobStatusesStore } from '../../core/stores/job-statuses.store';
import { jobLabel, JobsStore } from '../../core/stores/jobs.store';
import { PageHeaderComponent } from '../../shell/page-header.component';
import { openJobUrl, urlHost } from './job-url';
import { RunStatusBadgeComponent } from './run-status-badge.component';
import { StatusPickerComponent } from './status-picker.component';
import { StatusPillComponent } from './status-pill.component';

@Component({
  selector: 'app-jobs-page',
  imports: [PageHeaderComponent, RouterLink, ZardButtonComponent, ZardSkeletonComponent, RunStatusBadgeComponent, NgIcon, StatusPickerComponent, StatusPillComponent],
  viewProviders: [provideIcons({ lucidePlus })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'Jobs' }]">
      <a z-button zSize="sm" routerLink="/jobs/new"><ng-icon name="lucidePlus" /> New job</a>
    </app-page-header>
    <div class="flex flex-1 flex-col gap-4 p-4">
      @if (jobs.jobs().length) {
        <div class="flex flex-wrap items-center gap-1.5" role="group" aria-label="Filter by status">
          <button z-button zSize="xs" [zType]="filter() === 'all' ? 'secondary' : 'ghost'" (click)="filter.set('all')">
            All <span class="text-muted-foreground tabular-nums">{{ jobs.jobs().length }}</span>
          </button>
          @for (s of statuses.statuses(); track s.id) {
            <button z-button zSize="xs" [zType]="filter() === s.id ? 'secondary' : 'ghost'" (click)="filter.set(s.id)">
              <app-status-pill [name]="s.name" [color]="s.color" />
              <span class="text-muted-foreground tabular-nums">{{ counts().get(s.id) ?? 0 }}</span>
            </button>
          }
        </div>
      }
      @if (jobs.loading() && !jobs.jobs().length) {
        @for (i of [1, 2, 3]; track i) {
          <z-skeleton class="h-14 w-full" />
        }
      } @else {
        <ul class="divide-y rounded-xl border" aria-label="Jobs">
          @for (j of visible(); track j.id) {
            <li class="hover:bg-muted/50 flex items-center gap-3 pr-4">
              <a [routerLink]="['/jobs', j.id]" class="flex min-w-0 flex-1 items-center justify-between gap-3 py-3 pl-4">
                <div class="min-w-0">
                  <div class="truncate font-medium">{{ label(j) }}</div>
                  <div class="text-muted-foreground flex gap-1.5 text-xs">
                    {{ date(j.created_at) }}
                    @if (j.url) {
                      <span aria-hidden="true">·</span>
                      <button type="button" class="truncate hover:underline" [title]="j.url" (click)="$event.preventDefault(); open(j.url)">
                        {{ host(j.url) }}
                      </button>
                    }
                  </div>
                </div>
                <app-run-status-badge [run]="jobs.latestRuns()[j.id]" />
              </a>
              <app-status-picker class="w-32 shrink-0" [value]="j.status_id" (valueChange)="setStatus(j, $event)" />
            </li>
          } @empty {
            <li class="text-muted-foreground flex flex-col items-center gap-3 p-10 text-center text-sm">
              @if (filter() !== 'all' && jobs.jobs().length) {
                <span class="text-foreground font-medium">No jobs with this status</span>
              } @else {
              <span class="text-foreground font-medium">No jobs yet</span>
              Paste a job description to rank your library against it.
              <a z-button routerLink="/jobs/new"><ng-icon name="lucidePlus" /> New job</a>
              }
            </li>
          }
        </ul>
      }
    </div>
  `,
})
export class JobsPageComponent implements OnInit {
  protected readonly jobs = inject(JobsStore);
  protected readonly label = jobLabel;
  protected readonly statuses = inject(JobStatusesStore);
  private readonly toast = inject(ZardSonnerService);
  protected readonly date = formatTimestamp;
  protected readonly host = urlHost;
  protected readonly open = openJobUrl;

  protected readonly filter = signal<'all' | number>('all');
  protected readonly counts = computed(() => {
    const m = new Map<number, number>();
    for (const j of this.jobs.jobs()) if (j.status_id !== null) m.set(j.status_id, (m.get(j.status_id) ?? 0) + 1);
    return m;
  });
  protected readonly visible = computed(() => {
    const f = this.filter();
    return f === 'all' ? this.jobs.sorted() : this.jobs.sorted().filter((j) => j.status_id === f);
  });

  ngOnInit(): void {
    void this.jobs.load();
    void this.statuses.ensureLoaded();
  }

  protected async setStatus(j: JobDescription, statusId: number | null): Promise<void> {
    try {
      await this.jobs.setTracking(j.id, statusId, j.url);
    } catch (e) {
      this.toast.error('Could not update status', { description: messageFor(e) });
    }
  }
}
