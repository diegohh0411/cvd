import { ChangeDetectionStrategy, Component, computed, inject, OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { RouterLink } from '@angular/router';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucidePlus } from '@ng-icons/lucide';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardInputComponent } from '@/shared/components/input';
import { ZardSkeletonComponent } from '@/shared/components/skeleton/skeleton.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { JobDescription } from '../../core/ipc/types';
import { JobStatusesStore } from '../../core/stores/job-statuses.store';
import { JobsStore } from '../../core/stores/jobs.store';
import { ScanStore } from '../../core/stores/scan.store';
import { PageHeaderComponent } from '../../shell/page-header.component';
import { JobViewToolbarComponent } from './job-view-toolbar.component';
import { applyView, JobViewsStore } from './job-views';
import { JobsBoardComponent } from './jobs-board.component';
import { JobsTableComponent } from './jobs-table.component';

@Component({
  selector: 'app-jobs-page',
  imports: [FormsModule, ZardInputComponent, PageHeaderComponent, RouterLink, ZardButtonComponent, ZardSkeletonComponent, NgIcon, JobViewToolbarComponent, JobsTableComponent, JobsBoardComponent],
  viewProviders: [provideIcons({ lucidePlus })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'Jobs' }]">
      @if (scanStore.pendingCount() > 0) {
        <a z-button zType="outline" zSize="sm" data-testid="review-badge" routerLink="/inbox" [queryParams]="{ filter: 'review' }">
          {{ scanStore.pendingCount() }} to review
        </a>
      }
      <a z-button zSize="sm" routerLink="/jobs/new"><ng-icon name="lucidePlus" /> New job</a>
    </app-page-header>
    <div class="flex flex-1 flex-col gap-4 p-4">
      @if (jobs.jobs().length) {
        <input
          z-input
          type="search"
          class="max-w-sm"
          placeholder="Search jobs by company, title or site…"
          aria-label="Search jobs"
          data-testid="job-search"
          [ngModel]="query()"
          (ngModelChange)="search($event)"
        />
        @if (view()) {
          <app-job-view-toolbar />
        }
      }
      @if (jobs.loading() && !jobs.jobs().length) {
        @for (i of [1, 2, 3]; track i) {
          <z-skeleton class="h-14 w-full" />
        }
      } @else if (view()?.layout === 'board' && jobs.jobs().length) {
        <app-jobs-board [jobs]="visible()" (statusChange)="setStatus($event.job, $event.statusId)" />
      } @else {
        <app-jobs-table [jobs]="visible()" (statusChange)="setStatus($event.job, $event.statusId)">
          <div class="text-muted-foreground flex flex-col items-center gap-3 p-10 text-center text-sm">
            @if (hits() !== null) {
              <span class="text-foreground font-medium">No jobs match “{{ query().trim() }}”</span>
            } @else if (jobs.jobs().length) {
              <span class="text-foreground font-medium">No jobs match this view’s filters</span>
            } @else {
              <span class="text-foreground font-medium">No jobs yet</span>
              Paste a job description to rank your library against it.
              <a z-button routerLink="/jobs/new"><ng-icon name="lucidePlus" /> New job</a>
            }
          </div>
        </app-jobs-table>
      }
    </div>
  `,
})
export class JobsPageComponent implements OnInit {
  protected readonly jobs = inject(JobsStore);
  protected readonly statuses = inject(JobStatusesStore);
  private readonly toast = inject(ZardSonnerService);

  private readonly views = inject(JobViewsStore);
  protected readonly view = this.views.active;
  protected readonly query = signal('');
  /** Fuzzy-ranked job ids for the current query; null when not searching. */
  protected readonly hits = signal<number[] | null>(null);
  protected readonly visible = computed(() => {
    const ids = this.hits();
    const byId = new Map(this.jobs.jobs().map((j) => [j.id, j]));
    const base = ids === null ? this.jobs.sorted() : ids.flatMap((id) => byId.get(id) ?? []);
    const view = this.view();
    return view ? applyView(base, view, this.statuses.byId()) : base;
  });

  protected readonly scanStore = inject(ScanStore);

  ngOnInit(): void {
    void this.jobs.load();
    void this.statuses.ensureLoaded();
    void this.views.ensureLoaded();
    void this.scanStore.ensureReady();
    void this.scanStore.refreshPending();
  }

  protected async search(query: string): Promise<void> {
    this.query.set(query);
    if (!query.trim()) {
      this.hits.set(null);
      return;
    }
    try {
      const ids = await api.searchJobs(query);
      if (this.query() === query) this.hits.set(ids);
    } catch (e) {
      this.toast.error('Search failed', { description: messageFor(e) });
    }
  }

  protected async setStatus(j: JobDescription, statusId: number | null): Promise<void> {
    try {
      await this.jobs.setTracking(j.id, statusId, j.url);
    } catch (e) {
      this.toast.error('Could not update status', { description: messageFor(e) });
    }
  }
}
