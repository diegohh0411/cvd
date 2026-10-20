import { ChangeDetectionStrategy, Component, inject, input, output } from '@angular/core';
import { RouterLink } from '@angular/router';

import { formatTimestamp } from '../../core/format';
import type { JobDescription } from '../../core/ipc/types';
import { JobsStore } from '../../core/stores/jobs.store';
import { openJobUrl, urlHost } from './job-url';
import { RunStatusBadgeComponent } from './run-status-badge.component';
import { StatusPickerComponent } from './status-picker.component';

/** Notion-style table of jobs: one row per job, one column per property. */
@Component({
  selector: 'app-jobs-table',
  imports: [RouterLink, RunStatusBadgeComponent, StatusPickerComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'block overflow-x-auto rounded-xl border' },
  template: `
    <table class="w-full text-sm" aria-label="Jobs">
      <thead class="text-muted-foreground text-xs">
        <tr class="border-b">
          <th class="px-3 py-2 text-left font-normal">Company</th>
          <th class="px-3 py-2 text-left font-normal">Title</th>
          <th class="w-36 px-3 py-2 text-left font-normal">Status</th>
          <th class="px-3 py-2 text-left font-normal">Site</th>
          <th class="px-3 py-2 text-left font-normal whitespace-nowrap">Created</th>
          <th class="px-3 py-2 text-left font-normal">Match</th>
        </tr>
      </thead>
      <tbody class="divide-y">
        @for (j of jobs(); track j.id) {
          <tr class="hover:bg-muted/50">
            <td class="max-w-56 truncate px-3 py-1.5 font-medium">
              <a [routerLink]="['/jobs', j.id]" class="hover:underline">{{ j.company.trim() || 'Untitled' }}</a>
            </td>
            <td class="max-w-72 truncate px-3 py-1.5">
              <a [routerLink]="['/jobs', j.id]" class="hover:underline">{{ j.title }}</a>
            </td>
            <td class="px-2 py-1"><app-status-picker [value]="j.status_id" (valueChange)="statusChange.emit({ job: j, statusId: $event })" /></td>
            <td class="text-muted-foreground max-w-40 truncate px-3 py-1.5">
              @if (j.url) {
                <button type="button" class="hover:underline" [title]="j.url" (click)="open(j.url)">{{ host(j.url) }}</button>
              }
            </td>
            <td class="text-muted-foreground px-3 py-1.5 whitespace-nowrap">{{ date(j.created_at) }}</td>
            <td class="px-3 py-1.5"><app-run-status-badge [run]="store.latestRuns()[j.id]" /></td>
          </tr>
        } @empty {
          <tr>
            <td colspan="6" class="p-0"><ng-content /></td>
          </tr>
        }
      </tbody>
    </table>
  `,
})
export class JobsTableComponent {
  protected readonly store = inject(JobsStore);
  readonly jobs = input.required<JobDescription[]>();
  readonly statusChange = output<{ job: JobDescription; statusId: number | null }>();

  protected readonly date = formatTimestamp;
  protected readonly host = urlHost;
  protected readonly open = openJobUrl;
}
