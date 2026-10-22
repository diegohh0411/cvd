import { ChangeDetectionStrategy, Component, computed, effect, inject, input, OnDestroy, output, signal, untracked } from '@angular/core';
import { FormsModule } from '@angular/forms';

import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { messageFor } from '../../core/ipc/errors';
import type { JobDescription } from '../../core/ipc/types';
import { JobsStore } from '../../core/stores/jobs.store';
import { wordCount } from '../jobs/new-job.component';

/** Pause after the last keystroke before the description saves. */
export const AUTOSAVE_MS = 800;

/** The job description as an always-editable text box. Saves after a pause, on blur, and when it leaves the page. */
@Component({
  selector: 'app-jd-editor',
  imports: [FormsModule],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="flex min-h-0 flex-1 flex-col gap-1.5">
      <div class="flex items-baseline justify-between gap-2 text-xs">
        <span class="text-muted-foreground tabular-nums">{{ words() }} words</span>
        <span role="status" class="text-muted-foreground">{{ statusText() }}</span>
      </div>
      <textarea
        aria-label="Job description"
        class="border-input focus-visible:ring-ring/50 focus-visible:border-ring min-h-96 w-full flex-1 resize-y rounded-md border bg-transparent p-3 text-sm leading-relaxed outline-none focus-visible:ring-[3px]"
        placeholder="No description yet. Paste the job posting here…"
        [ngModel]="draft()"
        (ngModelChange)="onInput($event)"
        (blur)="flush()"
      ></textarea>
    </div>
  `,
})
export class JdEditorComponent implements OnDestroy {
  private readonly jobs = inject(JobsStore);
  private readonly toast = inject(ZardSonnerService);

  readonly job = input.required<JobDescription>();
  /** Emits the job with its saved description. */
  readonly changed = output<JobDescription>();

  protected readonly draft = signal('');
  /** Text the server has, so unchanged drafts are not sent again. */
  private readonly saved = signal('');
  private readonly status = signal<'idle' | 'saving' | 'error'>('idle');
  protected readonly words = computed(() => wordCount(this.draft()));
  protected readonly statusText = computed(() => {
    if (this.status() === 'saving') return 'Saving…';
    if (this.status() === 'error') return 'Could not save';
    return this.draft() === this.saved() ? 'Saved' : 'Unsaved changes';
  });

  private timer: ReturnType<typeof setTimeout> | undefined;
  /** Saves run one after another so a slow save cannot land after a newer one. */
  private queue: Promise<void> = Promise.resolve();
  private readonly jobId = computed(() => this.job().id);

  constructor() {
    // Reset only when the job itself changes, not when a save hands back a new object for the same job.
    effect(() => {
      this.jobId();
      untracked(() => {
        const text = this.job().raw_text;
        this.draft.set(text);
        this.saved.set(text);
        this.status.set('idle');
      });
    });
  }

  ngOnDestroy(): void {
    this.flush();
  }

  protected onInput(text: string): void {
    this.draft.set(text);
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.flush(), AUTOSAVE_MS);
  }

  /** Saves now if there is anything unsaved. */
  flush(): void {
    clearTimeout(this.timer);
    this.timer = undefined;
    this.queue = this.queue.then(() => this.persist());
  }

  private async persist(): Promise<void> {
    const text = this.draft();
    if (text === this.saved()) return;
    const job = this.job();
    this.status.set('saving');
    try {
      await this.jobs.setDescription(job.id, text);
      this.saved.set(text);
      this.status.set('idle');
      this.changed.emit({ ...job, raw_text: text });
    } catch (e) {
      this.status.set('error');
      this.toast.error('Could not save the job description', { description: messageFor(e) });
    }
  }
}
