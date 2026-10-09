import { ChangeDetectionStrategy, Component, computed, inject, input, OnInit, output } from '@angular/core';

import { ZardBadgeComponent } from '@/shared/components/badge/badge.component';
import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardProgressComponent } from '@/shared/components/progress/progress.component';
import type { ModelInfo } from '../../core/ipc/types';
import { EngineStore } from '../../core/stores/engine.store';
import { formatBytes, ModelsStore } from '../../core/stores/models.store';

export function eta(remaining: number, rate: number): string {
  if (!rate || remaining <= 0) return '';
  const s = Math.round(remaining / rate);
  return s < 60 ? `${s}s left` : s < 3600 ? `${Math.round(s / 60)} min left` : `${(s / 3600).toFixed(1)} h left`;
}

/** Model cards: RAM fit, download with pause/resume, and activation. */
@Component({
  selector: 'app-model-cards',
  imports: [ZardButtonComponent, ZardBadgeComponent, ZardProgressComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid gap-3">
      @for (m of list(); track m.spec.id) {
        @let d = models.downloads()[m.spec.id];
        @let ok = models.supports(m.spec);
        <div class="flex flex-col gap-2 rounded-lg border p-4" [class.opacity-60]="!ok" [attr.data-testid]="'model-' + m.spec.id">
          <div class="flex flex-wrap items-center gap-2">
            <span class="font-medium">{{ m.spec.label }}</span>
            @if (m.spec.id === recommendedId()) {
              <z-badge>Recommended</z-badge>
            }
            @if (m.spec.kind === 'chat') {
              <z-badge zType="outline">Company research</z-badge>
            }
            @if (m.spec.id === activeId() || m.spec.id === models.researchModel()) {
              <z-badge zType="secondary">Active</z-badge>
            } @else if (m.installed) {
              <z-badge zType="outline">Installed</z-badge>
            }
          </div>
          <div class="text-muted-foreground text-xs">
            {{ size(m.spec.size_bytes) }} download · needs {{ m.spec.min_ram_gb }} GB RAM
            @if (!ok) {
              · <span class="text-destructive">this computer has {{ models.ramGb() }} GB</span>
            }
          </div>
          @if (d && (d.status === 'downloading' || d.status === 'paused') && !m.installed) {
            <z-progress [value]="d.total ? (d.done / d.total) * 100 : 0" class="h-1.5" />
            <div class="text-muted-foreground flex justify-between text-xs tabular-nums" aria-live="polite">
              <span>{{ size(d.done) }} / {{ size(d.total || m.spec.size_bytes) }}</span>
              <span>{{ d.status === 'paused' ? 'Paused' : remaining(d.total - d.done, d.rate) }}</span>
            </div>
          }
          @if (d?.status === 'error') {
            <p class="text-destructive text-xs" role="alert">{{ d!.error }}</p>
          }
          <div class="flex gap-2">
            @if (m.installed) {
              @if (m.spec.kind === 'chat') {
                @if (m.spec.id !== models.researchModel()) {
                  <button z-button zSize="sm" (click)="models.useForResearch(m.spec.id)">Use for research</button>
                }
              } @else if (m.spec.id !== activeId()) {
                <button z-button zSize="sm" (click)="activate(m)">Use this model</button>
              }
            } @else if (d?.status === 'downloading') {
              <button z-button zType="outline" zSize="sm" (click)="models.pause(m.spec.id)">Pause</button>
            } @else {
              <button z-button zSize="sm" [zDisabled]="!ok" (click)="download(m)">
                {{ d?.status === 'paused' ? 'Resume' : d?.status === 'error' ? 'Retry' : 'Download' }}
              </button>
            }
          </div>
        </div>
      }
    </div>
  `,
})
export class ModelCardsComponent implements OnInit {
  /** Only installed models when true. */
  readonly installedOnly = input(false);
  /** Activate automatically when a download finishes. */
  readonly autoActivate = input(true);
  readonly activated = output<string>();
  protected readonly models = inject(ModelsStore);
  private readonly engine = inject(EngineStore);
  protected readonly size = formatBytes;
  protected readonly remaining = eta;
  protected readonly list = computed(() => (this.installedOnly() ? this.models.installed() : this.models.models()));
  protected readonly recommendedId = computed(() => this.models.recommended()?.id);
  protected readonly activeId = computed(() => this.engine.status().model_id);

  ngOnInit(): void {
    void this.models.loadResearchModel();
  }

  async download(m: ModelInfo): Promise<void> {
    const ok = await this.models.download(m.spec.id);
    if (!ok || !this.autoActivate()) return;
    if (m.spec.kind === 'chat') {
      if (!this.models.researchModel()) await this.models.useForResearch(m.spec.id);
    } else if (!this.activeId()) await this.activate(m);
  }

  async activate(m: ModelInfo): Promise<void> {
    await this.models.activate(m.spec.id);
    await this.engine.refresh();
    this.activated.emit(m.spec.id);
  }
}
