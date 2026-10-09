import { ChangeDetectionStrategy, Component, computed, effect, inject, input, signal, untracked } from '@angular/core';
import { Router, RouterLink } from '@angular/router';
import { NgIcon, provideIcons } from '@ng-icons/core';
import { lucideDownload, lucideRefreshCw } from '@ng-icons/lucide';

import { ZardBadgeComponent } from '@/shared/components/badge/badge.component';
import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSkeletonComponent } from '@/shared/components/skeleton/skeleton.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { toAppError } from '../../core/ipc/errors';
import type { AppError, Selection } from '../../core/ipc/types';
import { EngineStore } from '../../core/stores/engine.store';
import { jobLabel } from '../../core/stores/jobs.store';
import { ProfileStore } from '../../core/stores/profile.store';
import { PageHeaderComponent } from '../../shell/page-header.component';
import { BlockerAlertComponent } from '../jobs/blocker-alert.component';
import { JobPropertiesComponent } from '../jobs/job-properties.component';
import { MatchReadiness } from '../jobs/match-readiness';
import { WorkspaceCardsComponent } from './cards.component';
import { defaultFileName, ExportService } from './export.service';
import { ExportsTabComponent } from './exports-tab.component';
import { MatchingStepperComponent } from './matching-stepper.component';
import { PdfPreviewComponent } from './pdf-preview.component';
import { SelectionListComponent } from './selection-list.component';
import { WorkspaceStore } from './workspace.store';

export type Tab = 'jd' | 'selection' | 'exports';
const TABS: { id: Tab; label: string }[] = [
  { id: 'jd', label: 'JD' },
  { id: 'selection', label: 'Selection' },
  { id: 'exports', label: 'Exports' },
];

/** What to tell the user about a failed run, and whether Retry makes sense. */
export function failureCopy(e: AppError | null): { title: string; body: string } {
  switch (e?.kind) {
    case 'ContextOverflow':
      return { title: 'The job description is too long for this model', body: 'Trim boilerplate (benefits, legal text) and match again.' };
    case 'EngineUnavailable':
      return { title: 'The decision engine stopped', body: e.message };
    case 'ModelMissing':
      return { title: 'No model available', body: e.message };
    default:
      return { title: 'Matching failed', body: e?.message ?? 'The run did not finish. Your partial scores are kept.' };
  }
}

@Component({
  selector: 'app-workspace-page',
  imports: [
    PageHeaderComponent,
    RouterLink,
    ZardBadgeComponent,
    ZardButtonComponent,
    ZardSkeletonComponent,
    NgIcon,
    MatchingStepperComponent,
    WorkspaceCardsComponent,
    SelectionListComponent,
    PdfPreviewComponent,
    ExportsTabComponent,
    BlockerAlertComponent,
    JobPropertiesComponent,
  ],
  providers: [WorkspaceStore],
  viewProviders: [provideIcons({ lucideDownload, lucideRefreshCw })],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="crumbs()">
      @if (ws.phase() === 'review') {
        @if (confirmRematch()) {
          <span class="text-muted-foreground hidden text-xs sm:inline">Discard your edits?</span>
          <button z-button zType="ghost" zSize="sm" (click)="confirmRematch.set(false)">Keep</button>
          <button z-button zType="destructive" zSize="sm" (click)="rematch(true)">Re-match</button>
        } @else {
          <button z-button zType="outline" zSize="sm" (click)="rematch(false)" [zDisabled]="!!readiness.blocker()">
            <ng-icon name="lucideRefreshCw" /> Re-match
          </button>
        }
        <button z-button zSize="sm" (click)="export()" [zDisabled]="!ws.selection() || ws.fitting()" [zLoading]="exporting()">
          <ng-icon name="lucideDownload" /> Export PDF
        </button>
      }
    </app-page-header>

    <div class="flex flex-1 flex-col gap-4 p-4">
      @if (ws.loadError(); as e) {
        <div role="alert" class="rounded-lg border p-6 text-sm">
          {{ e.message }} <a routerLink="/jobs" class="underline">Back to jobs</a>
        </div>
      } @else {
        @if (ws.phase() === 'loading') {
          <z-skeleton class="h-40 w-full" />
        }
        @if (ws.job(); as job) {
          <app-job-properties [job]="job" (changed)="ws.job.set($event)" />
        }
        @switch (ws.phase()) {
          @case ('new') {
            <div class="mx-auto flex w-full max-w-md flex-col gap-4 rounded-xl border p-6">
              <h2 class="font-semibold">Ready to match</h2>
              <p class="text-muted-foreground text-sm">The local model will rank your library against this job description.</p>
              @if (readiness.blocker(); as b) {
                <app-blocker-alert [blocker]="b" />
              }
              <button z-button (click)="start()" [zDisabled]="!!readiness.blocker()">Match</button>
            </div>
          }
          @case ('matching') {
            <app-matching-stepper [progress]="ws.progress()" [startedAt]="ws.startedAt()" (cancel)="cancel()" />
          }
          @case ('review') {
            <app-workspace-cards />
          }
          @default {
            <div role="alert" class="mx-auto flex w-full max-w-lg flex-col gap-3 rounded-xl border p-6">
              @if (ws.phase() === 'cancelled') {
                <h2 class="font-semibold">Matching cancelled</h2>
                <p class="text-muted-foreground text-sm">Scores computed so far are kept. Resume to finish the rest.</p>
              } @else {
                <h2 class="font-semibold">{{ failure().title }}</h2>
                <p class="text-muted-foreground text-sm whitespace-pre-wrap">{{ failure().body }}</p>
                @if (engine.status().error; as log) {
                  @if (log !== failure().body) {
                    <pre class="bg-muted max-h-40 overflow-auto rounded p-2 text-xs">{{ log }}</pre>
                  }
                }
              }
              <div class="flex gap-2">
                <button z-button (click)="resume()">{{ ws.phase() === 'cancelled' ? 'Resume' : 'Retry' }}</button>
                <button z-button zType="outline" (click)="rematch(true)">Start over</button>
              </div>
            </div>
          }
        }

        @if (ws.phase() !== 'loading') {
          <div class="grid min-h-0 flex-1 gap-4" [class.xl:grid-cols-2]="ws.phase() === 'review'">
            <div class="flex min-w-0 flex-col gap-3">
              <div role="tablist" aria-label="Workspace" class="bg-muted text-muted-foreground inline-flex w-fit rounded-lg p-1">
                @for (t of tabs; track t.id) {
                  <button
                    role="tab"
                    type="button"
                    class="rounded-md px-3 py-1 text-sm font-medium transition-colors"
                    [class]="tab() === t.id ? 'bg-background text-foreground shadow-sm' : 'hover:text-foreground'"
                    [attr.aria-selected]="tab() === t.id"
                    [disabled]="t.id === 'selection' && ws.phase() !== 'review'"
                    (click)="tab.set(t.id)"
                  >
                    {{ t.label }}
                  </button>
                }
              </div>
              <div role="tabpanel" class="min-h-0 flex-1 overflow-y-auto">
                @switch (tab()) {
                  @case ('jd') {
                    @if (ws.ranked()?.tags?.length) {
                      <div class="mb-4 flex flex-wrap gap-1.5" aria-label="Role focus">
                        @for (tag of ws.ranked()!.tags; track tag) {
                          <z-badge zType="secondary">{{ tag }}</z-badge>
                        }
                      </div>
                    }
                    <article class="text-sm leading-relaxed whitespace-pre-wrap">{{ ws.job()?.raw_text }}</article>
                  }
                  @case ('selection') {
                    <app-selection-list />
                  }
                  @case ('exports') {
                    <app-exports-tab [jobId]="jobIdNum()" [version]="exports()" (restore)="restore($event)" />
                  }
                }
              </div>
            </div>
            @if (ws.phase() === 'review') {
              <div class="min-w-0 xl:sticky xl:top-16 xl:h-[calc(100vh-5rem)]">
                <app-pdf-preview [pdf]="ws.pdf()" [error]="ws.fitError()" [busy]="ws.fitting()" />
              </div>
            }
          </div>
        }
      }
    </div>
  `,
})
export class WorkspacePageComponent {
  /** Route param. */
  readonly jobId = input.required<string>();
  /** `?start=1` from New job: start matching once the listener is up. */
  readonly start_ = input<string | undefined>(undefined, { alias: 'start' });
  /** `?tab=exports` deep link. */
  readonly tabParam = input<string | undefined>(undefined, { alias: 'tab' });

  protected readonly ws = inject(WorkspaceStore);
  protected readonly engine = inject(EngineStore);
  protected readonly readiness = inject(MatchReadiness);
  private readonly profile = inject(ProfileStore);
  private readonly exporter = inject(ExportService);
  private readonly toast = inject(ZardSonnerService);
  private readonly router = inject(Router);

  protected readonly tabs = TABS;
  protected readonly tab = signal<Tab>('jd');
  protected readonly exporting = signal(false);
  protected readonly exports = signal(0);
  protected readonly confirmRematch = signal(false);
  protected readonly jobIdNum = computed(() => Number(this.jobId()));
  protected readonly failure = computed(() => failureCopy(this.ws.runError()));
  protected readonly crumbs = computed(() => {
    const job = this.ws.job();
    return [{ label: 'Jobs', link: '/jobs' }, { label: job ? jobLabel(job) : '…' }];
  });

  constructor() {
    effect(() => {
      const id = this.jobIdNum();
      const autoStart = this.start_() === '1';
      untracked(() => {
        if (this.tabParam() === 'exports') this.tab.set('exports');
        void this.ws.init(id, autoStart).then(() => {
          if (autoStart) void this.router.navigate([], { queryParams: { start: null }, queryParamsHandling: 'merge', replaceUrl: true });
        });
      });
    });
    // Land on the selection once a review is available.
    effect(() => {
      if (this.ws.phase() === 'review' && untracked(this.tab) === 'jd' && untracked(this.tabParam) !== 'exports') this.tab.set('selection');
    });
  }

  async start(): Promise<void> {
    try {
      await this.ws.start();
    } catch (e) {
      this.onStartError(e);
    }
  }

  async rematch(confirmed: boolean): Promise<void> {
    if (!confirmed && this.ws.edited()) {
      this.confirmRematch.set(true);
      return;
    }
    this.confirmRematch.set(false);
    this.tab.set('jd');
    await this.start();
  }

  async resume(): Promise<void> {
    await this.ws.resume();
  }

  async cancel(): Promise<void> {
    try {
      await this.ws.cancel();
    } catch (e) {
      this.toast.error('Could not cancel', { description: toAppError(e).message });
    }
  }

  async export(): Promise<void> {
    const run = this.ws.run();
    const sel = this.ws.selection();
    const job = this.ws.job();
    if (!run || !sel || !job) return;
    this.exporting.set(true);
    try {
      if (await this.exporter.export(run.id, sel, defaultFileName(this.profile.profile().name, job))) this.exports.update((n) => n + 1);
    } finally {
      this.exporting.set(false);
    }
  }

  restore(sel: Selection): void {
    this.ws.restore(sel);
    this.tab.set('selection');
    this.toast.show('Selection restored');
  }

  private onStartError(e: unknown): void {
    const err = toAppError(e);
    if (err.kind === 'ModelMissing') void this.router.navigate(['/models'], { queryParams: { tab: 'download' } });
    this.toast.error('Could not start matching', { description: err.message });
  }
}
