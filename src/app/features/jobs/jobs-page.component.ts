import { ChangeDetectionStrategy, Component, inject, OnInit } from '@angular/core';
import { RouterLink } from '@angular/router';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucidePlus } from '@ng-icons/lucide';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSkeletonComponent } from '@/shared/components/skeleton/skeleton.component';
import { formatTimestamp } from '../../core/format';
import { jobLabel, JobsStore } from '../../core/stores/jobs.store';
import { PageHeaderComponent } from '../../shell/page-header.component';
import { RunStatusBadgeComponent } from './run-status-badge.component';

@Component({
  selector: 'app-jobs-page',
  imports: [PageHeaderComponent, RouterLink, ZardButtonComponent, ZardSkeletonComponent, RunStatusBadgeComponent, NgIcon],
  viewProviders: [provideIcons({ lucidePlus })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'Jobs' }]">
      <a z-button zSize="sm" routerLink="/jobs/new"><ng-icon name="lucidePlus" /> New job</a>
    </app-page-header>
    <div class="flex flex-1 flex-col gap-4 p-4">
      @if (jobs.loading() && !jobs.jobs().length) {
        @for (i of [1, 2, 3]; track i) {
          <z-skeleton class="h-14 w-full" />
        }
      } @else {
        <ul class="divide-y rounded-xl border" aria-label="Jobs">
          @for (j of jobs.sorted(); track j.id) {
            <li>
              <a [routerLink]="['/jobs', j.id]" class="hover:bg-muted/50 flex items-center justify-between gap-3 px-4 py-3">
                <div class="min-w-0">
                  <div class="truncate font-medium">{{ label(j) }}</div>
                  <div class="text-muted-foreground text-xs">{{ date(j.created_at) }}</div>
                </div>
                <app-run-status-badge [run]="jobs.latestRuns()[j.id]" />
              </a>
            </li>
          } @empty {
            <li class="text-muted-foreground flex flex-col items-center gap-3 p-10 text-center text-sm">
              <span class="text-foreground font-medium">No jobs yet</span>
              Paste a job description to rank your library against it.
              <a z-button routerLink="/jobs/new"><ng-icon name="lucidePlus" /> New job</a>
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
  protected readonly date = formatTimestamp;

  ngOnInit(): void {
    void this.jobs.load();
  }
}
