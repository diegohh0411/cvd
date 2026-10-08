import { ChangeDetectionStrategy, Component, effect, inject, input, output, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucideCircleDot, lucideExternalLink, lucideLink } from '@ng-icons/lucide';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { messageFor } from '../../core/ipc/errors';
import type { JobDescription } from '../../core/ipc/types';
import { JobsStore } from '../../core/stores/jobs.store';
import { isOpenableUrl, normalizeUrl, openJobUrl } from './job-url';
import { StatusPickerComponent } from './status-picker.component';

/** Notion-style property rows (Status, URL) for one job; saves on change. */
@Component({
  selector: 'app-job-properties',
  imports: [FormsModule, NgIcon, ZardButtonComponent, StatusPickerComponent],
  viewProviders: [provideIcons({ lucideCircleDot, lucideExternalLink, lucideLink })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <dl class="grid grid-cols-[7rem_minmax(0,1fr)] items-center gap-x-2 gap-y-1 text-sm">
      <dt class="text-muted-foreground flex items-center gap-1.5"><ng-icon name="lucideCircleDot" /> Status</dt>
      <dd><app-status-picker [value]="job().status_id" placeholder="Empty" (valueChange)="save($event, job().url)" /></dd>
      <dt class="text-muted-foreground flex items-center gap-1.5"><ng-icon name="lucideLink" /> URL</dt>
      <dd class="flex min-w-0 items-center gap-1">
        <input
          type="url"
          aria-label="Job posting URL"
          placeholder="Empty"
          class="hover:bg-muted focus:bg-muted placeholder:text-muted-foreground h-7 min-w-0 flex-1 truncate rounded-md bg-transparent px-2 text-sm outline-none"
          [ngModel]="draft()"
          (ngModelChange)="draft.set($event)"
          (keydown.enter)="$any($event.target).blur()"
          (blur)="saveUrl()"
        />
        @if (openable(job().url)) {
          <button z-button zType="ghost" zSize="icon-xs" type="button" aria-label="Open posting" (click)="open(job().url)">
            <ng-icon name="lucideExternalLink" />
          </button>
        }
      </dd>
    </dl>
  `,
})
export class JobPropertiesComponent {
  private readonly jobs = inject(JobsStore);
  private readonly toast = inject(ZardSonnerService);

  readonly job = input.required<JobDescription>();
  /** Emits the job with its saved tracking fields. */
  readonly changed = output<JobDescription>();

  protected readonly draft = signal('');
  protected readonly openable = isOpenableUrl;
  protected readonly open = openJobUrl;

  constructor() {
    effect(() => this.draft.set(this.job().url));
  }

  protected saveUrl(): void {
    const url = normalizeUrl(this.draft());
    if (url === this.job().url) return this.draft.set(url);
    void this.save(this.job().status_id, url);
  }

  protected async save(statusId: number | null, url: string): Promise<void> {
    const job = this.job();
    try {
      await this.jobs.setTracking(job.id, statusId, url);
      this.changed.emit({ ...job, status_id: statusId, url });
    } catch (e) {
      this.draft.set(job.url);
      this.toast.error('Could not save', { description: messageFor(e) });
    }
  }
}
