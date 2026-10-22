import { CdkDrag, type CdkDragDrop, CdkDropList, CdkDropListGroup } from '@angular/cdk/drag-drop';
import { ChangeDetectionStrategy, Component, computed, inject, input, output } from '@angular/core';
import { RouterLink } from '@angular/router';

import { formatTimestamp } from '../../core/format';
import type { JobDescription, StatusColor } from '../../core/ipc/types';
import { JobStatusesStore } from '../../core/stores/job-statuses.store';
import { jobLabel, JobsStore } from '../../core/stores/jobs.store';
import { urlHost } from './job-url';
import { RunStatusBadgeComponent } from './run-status-badge.component';
import { StatusPillComponent } from './status-pill.component';

interface Column {
  statusId: number | null;
  name: string;
  color: StatusColor;
  jobs: JobDescription[];
}

/** Kanban board grouped by status; drag a card to another column to change its status. */
@Component({
  selector: 'app-jobs-board',
  imports: [RouterLink, CdkDropListGroup, CdkDropList, CdkDrag, RunStatusBadgeComponent, StatusPillComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'block overflow-x-auto pb-2' },
  template: `
    <div class="flex items-start gap-3" cdkDropListGroup aria-label="Jobs board">
      @for (c of columns(); track c.statusId) {
        <section class="bg-muted/40 flex w-64 shrink-0 flex-col gap-2 rounded-xl p-2" [attr.aria-label]="c.name">
          <header class="flex items-center gap-2 px-1">
            <app-status-pill [name]="c.name" [color]="c.color" />
            <span class="text-muted-foreground text-xs tabular-nums">{{ c.jobs.length }}</span>
          </header>
          <ul class="flex min-h-12 flex-col gap-2" cdkDropList [cdkDropListData]="c.statusId" (cdkDropListDropped)="drop($event)">
            @for (j of c.jobs; track j.id) {
              <li cdkDrag [cdkDragData]="j" class="bg-background hover:bg-muted/50 cursor-grab rounded-lg border shadow-xs">
                <a [routerLink]="['/jobs', j.id]" class="flex flex-col gap-1 p-2.5" draggable="false">
                  <span class="text-sm font-medium">{{ label(j) }}</span>
                  <span class="text-muted-foreground flex items-center gap-1.5 text-xs">
                    {{ date(j.created_at) }}
                    @if (j.url) {
                      <span aria-hidden="true">·</span><span class="truncate">{{ host(j.url) }}</span>
                    }
                  </span>
                  <app-run-status-badge class="self-start" [run]="store.latestRuns()[j.id]" />
                </a>
              </li>
            }
          </ul>
        </section>
      }
    </div>
  `,
})
export class JobsBoardComponent {
  protected readonly store = inject(JobsStore);
  private readonly statuses = inject(JobStatusesStore);
  readonly jobs = input.required<JobDescription[]>();
  readonly statusChange = output<{ job: JobDescription; statusId: number | null }>();

  protected readonly label = jobLabel;
  protected readonly date = formatTimestamp;
  protected readonly host = urlHost;

  /** One column per status in the user's order, plus "No status" when any job lacks one. */
  protected readonly columns = computed<Column[]>(() => {
    const jobs = this.jobs();
    const cols: Column[] = this.statuses.statuses().map((s) => ({
      statusId: s.id,
      name: s.name,
      color: s.color,
      jobs: jobs.filter((j) => j.status_id === s.id),
    }));
    const known = new Set(cols.map((c) => c.statusId));
    const none = jobs.filter((j) => j.status_id === null || !known.has(j.status_id));
    return none.length || !cols.length ? [{ statusId: null, name: 'No status', color: 'gray', jobs: none }, ...cols] : cols;
  });

  protected drop(e: CdkDragDrop<number | null, number | null, JobDescription>): void {
    if (e.container.data !== e.previousContainer.data) this.statusChange.emit({ job: e.item.data, statusId: e.container.data });
  }
}
