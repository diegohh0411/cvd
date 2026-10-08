import { ChangeDetectionStrategy, Component, computed, inject, OnInit, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { revealItemInDir } from '@tauri-apps/plugin-opener';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { formatTimestamp } from '../../core/format';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { Cv } from '../../core/ipc/types';
import { jobLabel, JobsStore } from '../../core/stores/jobs.store';
import { PageHeaderComponent } from '../../shell/page-header.component';

export function fileName(path: string | null): string {
  return path ? (path.split(/[\\/]/).pop() ?? path) : '—';
}

@Component({
  selector: 'app-exports-page',
  imports: [PageHeaderComponent, RouterLink, ZardButtonComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'Jobs', link: '/jobs' }, { label: 'Exports' }]" />
    <div class="flex flex-1 flex-col gap-4 p-4">
      <ul class="divide-y rounded-xl border" aria-label="Exported CVs">
        @for (c of rows(); track c.cv.id) {
          <li class="flex flex-wrap items-center justify-between gap-3 px-4 py-3">
            <div class="min-w-0">
              <a class="font-medium hover:underline" [routerLink]="['/jobs', c.cv.jd_id]" [queryParams]="{ tab: 'exports' }">{{ c.job }}</a>
              <div class="text-muted-foreground truncate text-xs">
                {{ date(c.cv.created_at) }} · {{ c.cv.page_budget }} page{{ c.cv.page_budget === 1 ? '' : 's' }} · {{ file(c.cv.pdf_path) }}
              </div>
            </div>
            @if (c.cv.pdf_path) {
              <button z-button zType="outline" zSize="sm" (click)="reveal(c.cv.pdf_path)">Show in folder</button>
            }
          </li>
        } @empty {
          <li class="text-muted-foreground p-10 text-center text-sm">No exports yet. Export a CV from a job's review screen.</li>
        }
      </ul>
    </div>
  `,
})
export class ExportsPageComponent implements OnInit {
  private readonly jobs = inject(JobsStore);
  private readonly toast = inject(ZardSonnerService);
  private readonly cvs = signal<Cv[]>([]);
  protected readonly date = formatTimestamp;
  protected readonly file = fileName;
  protected readonly rows = computed(() => {
    const byId = new Map(this.jobs.jobs().map((j) => [j.id, jobLabel(j)]));
    return this.cvs().map((cv) => ({ cv, job: byId.get(cv.jd_id) ?? `Job ${cv.jd_id}` }));
  });

  async ngOnInit(): Promise<void> {
    await this.jobs.load();
    const all = await Promise.all(this.jobs.jobs().map((j) => api.listCvs(j.id).catch(() => [] as Cv[])));
    this.cvs.set(all.flat().sort((a, b) => b.id - a.id));
  }

  async reveal(path: string): Promise<void> {
    try {
      await revealItemInDir(path);
    } catch (e) {
      this.toast.error('Could not open the folder', { description: messageFor(e) });
    }
  }
}
