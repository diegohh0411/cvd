import { ChangeDetectionStrategy, Component, computed, inject, type OnInit, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { Router } from '@angular/router';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardInputComponent } from '@/shared/components/input/input.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor, toAppError } from '../../core/ipc/errors';
import { JobStatusesStore } from '../../core/stores/job-statuses.store';
import { JobsStore } from '../../core/stores/jobs.store';
import { PageHeaderComponent } from '../../shell/page-header.component';
import { BlockerAlertComponent } from './blocker-alert.component';
import { normalizeUrl } from './job-url';
import { StatusPickerComponent } from './status-picker.component';
import { MatchReadiness } from './match-readiness';

export function wordCount(text: string): number {
  return text.trim() ? text.trim().split(/\s+/).length : 0;
}

@Component({
  selector: 'app-new-job',
  imports: [PageHeaderComponent, FormsModule, ZardButtonComponent, ZardInputComponent, BlockerAlertComponent, StatusPickerComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'Jobs', link: '/jobs' }, { label: 'New job' }]" />
    <form class="mx-auto flex w-full max-w-3xl flex-1 flex-col gap-4 p-4" (ngSubmit)="submit()">
      @if (readiness.blocker(); as b) {
        <app-blocker-alert [blocker]="b" />
      }
      <div class="grid gap-3 sm:grid-cols-2">
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="jd-company">Company</label>
          <input z-input id="jd-company" name="company" [(ngModel)]="company" placeholder="Acme" />
        </div>
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="jd-title">Role title</label>
          <input z-input id="jd-title" name="title" [(ngModel)]="title" placeholder="Senior Backend Engineer" />
        </div>
      </div>
      <div class="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto]">
        <div class="grid gap-1.5">
          <label class="text-sm font-medium" for="jd-url">Posting URL</label>
          <input z-input id="jd-url" name="url" type="url" [(ngModel)]="url" placeholder="https://acme.com/careers/123" />
        </div>
        <div class="grid gap-1.5">
          <span class="text-sm font-medium" id="jd-status">Status</span>
          <app-status-picker aria-labelledby="jd-status" [value]="statusId()" (valueChange)="statusId.set($event)" />
        </div>
      </div>
      <div class="flex flex-1 flex-col gap-1.5">
        <div class="flex items-baseline justify-between">
          <label class="text-sm font-medium" for="jd-text">Job description</label>
          <span class="text-muted-foreground text-xs tabular-nums">{{ words() }} words</span>
        </div>
        <textarea
          id="jd-text"
          name="text"
          class="border-input focus-visible:ring-ring/50 focus-visible:border-ring min-h-72 flex-1 resize-none rounded-md border bg-transparent p-3 text-sm outline-none focus-visible:ring-[3px]"
          [placeholder]="fetchMode() ? 'Leave empty to read the description from the posting URL…' : 'Paste the full job description here…'"
          [(ngModel)]="text"
        ></textarea>
      </div>
      <div class="flex justify-end gap-2">
        <button z-button type="submit" [zDisabled]="!canSubmit()" [zLoading]="busy()">
          {{ fetchMode() ? 'Fetch description' : 'Match' }}
        </button>
      </div>
    </form>
  `,
})
export class NewJobComponent implements OnInit {
  private readonly jobs = inject(JobsStore);
  private readonly router = inject(Router);
  private readonly toast = inject(ZardSonnerService);
  protected readonly readiness = inject(MatchReadiness);
  private readonly statuses = inject(JobStatusesStore);

  protected readonly company = signal('');
  protected readonly title = signal('');
  protected readonly text = signal('');
  protected readonly url = signal('');
  protected readonly statusId = signal<number | null>(null);
  protected readonly busy = signal(false);
  protected readonly words = computed(() => wordCount(this.text()));
  protected readonly canMatch = computed(() => !this.busy() && !this.readiness.blocker() && this.words() > 0);
  /** No description pasted but a URL given: the button reads the posting instead of matching. */
  protected readonly fetchMode = computed(() => this.words() === 0 && this.url().trim() !== '');
  protected readonly canSubmit = computed(() => (this.fetchMode() ? !this.busy() : this.canMatch()));

  ngOnInit(): void {
    // Preselect the first status (e.g. Bookmarked), as the backend would.
    void this.statuses.ensureLoaded().then(() => {
      if (this.statusId() === null) this.statusId.set(this.statuses.statuses()[0]?.id ?? null);
    });
  }

  protected async submit(): Promise<void> {
    if (this.fetchMode()) await this.readPosting();
    else await this.match();
  }

  /** Fills the description from the posting so the user can review it before matching. */
  protected async readPosting(): Promise<void> {
    if (!this.canSubmit()) return;
    this.busy.set(true);
    try {
      this.text.set(await api.fetchJobDescription(normalizeUrl(this.url())));
    } catch (e) {
      this.toast.error('Could not read the posting', { description: messageFor(e) });
    } finally {
      this.busy.set(false);
    }
  }

  async match(): Promise<void> {
    if (!this.canMatch()) return;
    this.busy.set(true);
    let jobId: number | null = null;
    try {
      jobId = await this.jobs.create(
        this.title().trim(),
        this.company().trim(),
        this.text(),
        normalizeUrl(this.url()),
        this.statusId(),
      );
      // The workspace subscribes to progress before starting the run.
      await this.router.navigate(['/jobs', jobId], { queryParams: { start: 1 } });
    } catch (e) {
      const err = toAppError(e);
      if (err.kind === 'ModelMissing') {
        await this.router.navigate(['/ai/models']);
      }
      this.toast.error('Could not start matching', { description: err.message });
    } finally {
      this.busy.set(false);
    }
  }
}

