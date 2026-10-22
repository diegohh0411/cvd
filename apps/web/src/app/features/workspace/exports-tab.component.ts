import { ChangeDetectionStrategy, Component, DestroyRef, effect, inject, input, output, signal } from '@angular/core';
import { platform } from '../../core/platform/runtime';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { formatTimestamp } from '../../core/format';
import * as api from '../../core/ipc/commands';
import { onTablesChanged, untilDestroyed } from '../../core/ipc/events';
import type { Cv, Selection } from '../../core/ipc/types';
import { fileName } from '../jobs/exports-page.component';

@Component({
  selector: 'app-exports-tab',
  imports: [ZardButtonComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <ul class="divide-y rounded-lg border" aria-label="Exports for this job">
      @for (c of cvs(); track c.id) {
        <li class="flex flex-wrap items-center justify-between gap-2 px-3 py-2">
          <div class="min-w-0 text-sm">
            <div class="truncate font-medium">{{ file(c.file_ref) }}</div>
            <div class="text-muted-foreground text-xs">{{ date(c.created_at) }} · {{ c.page_budget }} page{{ c.page_budget === 1 ? '' : 's' }}</div>
          </div>
          <div class="flex gap-2">
            <button z-button zType="ghost" zSize="sm" (click)="restore.emit(parse(c))">Restore selection</button>
            @if (capabilities.revealInFolder && c.file_ref) {
              <button z-button zType="outline" zSize="sm" (click)="reveal(c.file_ref)">Show in folder</button>
            }
          </div>
        </li>
      } @empty {
        <li class="text-muted-foreground p-6 text-center text-sm">No exports for this job yet.</li>
      }
    </ul>
  `,
})
export class ExportsTabComponent {
  readonly jobId = input.required<number>();
  /** Bump to reload after an export. */
  readonly version = input(0);
  readonly restore = output<Selection>();
  protected readonly cvs = signal<Cv[]>([]);
  protected readonly capabilities = platform.capabilities;
  protected readonly date = formatTimestamp;
  protected readonly file = fileName;

  /** Bumped when the table changes elsewhere. */
  private readonly changes = signal(0);

  constructor() {
    void untilDestroyed(inject(DestroyRef), onTablesChanged(['cv'], () => this.changes.update((n) => n + 1))).catch(() => undefined);
    effect(() => {
      this.changes();
      const id = this.jobId();
      this.version();
      void api.listCvs(id).then((c) => this.cvs.set(c), () => this.cvs.set([]));
    });
  }

  parse(c: Cv): Selection {
    return JSON.parse(c.selection_json) as Selection;
  }

  reveal(path: string): void {
    void platform.reveal(path).catch(() => undefined);
  }
}
