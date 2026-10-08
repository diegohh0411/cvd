import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { ProviderModels, RemoteModel } from '../../core/ipc/types';
import { EngineStore } from '../../core/stores/engine.store';
import { ModelsStore } from '../../core/stores/models.store';

export const REMOTE_NOTICE_SETTING = 'ui.remote_notice_ack';

export function formatPrice(perMtok: number | null): string {
  if (perMtok === null) return '';
  if (perMtok === 0) return 'free';
  return `$${Number(perMtok.toPrecision(3))} / M tokens`;
}

@Component({
  selector: 'app-remote-models',
  imports: [ZardButtonComponent, RouterLink],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (!hasKey()) {
      <div class="text-muted-foreground rounded-xl border border-dashed p-10 text-center text-sm">
        Add an OpenRouter key in <a routerLink="/settings" class="underline">Settings</a> to use online decision models.
      </div>
    } @else {
      @if (pending(); as m) {
        <div class="grid gap-3 rounded-lg border p-4 text-sm" role="alertdialog" aria-label="Online model notice">
          <p>Your CV entries and job descriptions will be sent to OpenRouter and the model's provider to score them.</p>
          <div class="flex gap-2">
            <button z-button zSize="sm" (click)="confirm(m)">Use {{ m.display_name }}</button>
            <button z-button zType="outline" zSize="sm" (click)="pending.set(null)">Cancel</button>
          </div>
        </div>
      }
      @for (p of lists(); track p.provider) {
        <section class="grid gap-2">
          <div class="flex items-center justify-between">
            <h2 class="font-semibold">OpenRouter (online)</h2>
            <button z-button zType="outline" zSize="sm" [zLoading]="loading()" (click)="load(true)">Refresh</button>
          </div>
          @if (p.error) {
            <p class="text-destructive text-sm" role="alert">{{ p.error }}</p>
          }
          <ul class="divide-y rounded-lg border">
            @for (m of p.models; track m.id) {
              <li class="flex items-center gap-3 p-3 text-sm">
                <div class="grid flex-1 gap-0.5">
                  <span class="font-medium">{{ m.display_name }}</span>
                  <span class="text-muted-foreground text-xs">{{ m.id }} · {{ price(m.price_per_mtok_in) }}</span>
                </div>
                @if (engine.status().model_id === 'openrouter:' + m.id) {
                  <span class="text-xs text-emerald-600">Active</span>
                } @else {
                  <button z-button zType="outline" zSize="sm" (click)="choose(m)">Use</button>
                }
              </li>
            }
          </ul>
        </section>
      }
    }
  `,
})
export class RemoteModelsComponent implements OnInit {
  protected readonly engine = inject(EngineStore);
  private readonly models = inject(ModelsStore);
  private readonly toast = inject(ZardSonnerService);
  protected readonly hasKey = signal(false);
  protected readonly lists = signal<ProviderModels[]>([]);
  protected readonly loading = signal(false);
  protected readonly pending = signal<RemoteModel | null>(null);
  protected readonly price = formatPrice;

  ngOnInit(): void {
    void api.providerKeyStatus('openrouter').then(
      (s) => {
        this.hasKey.set(s.has_key);
        if (s.has_key) void this.load(false);
      },
      () => undefined,
    );
  }

  async load(refresh: boolean): Promise<void> {
    this.loading.set(true);
    try {
      this.lists.set(await api.listRemoteModels(refresh));
    } catch (e) {
      this.toast.error('Could not load online models', { description: messageFor(e) });
    } finally {
      this.loading.set(false);
    }
  }

  /** First online pick asks for consent before any data leaves the machine. */
  async choose(m: RemoteModel): Promise<void> {
    const ack = await api.getUiSetting(REMOTE_NOTICE_SETTING).catch(() => null);
    if (ack === '1') await this.activate(m);
    else this.pending.set(m);
  }

  async confirm(m: RemoteModel): Promise<void> {
    this.pending.set(null);
    await api.setUiSetting(REMOTE_NOTICE_SETTING, '1').catch(() => undefined);
    await this.activate(m);
  }

  private async activate(m: RemoteModel): Promise<void> {
    try {
      await this.models.activate(`openrouter:${m.id}`);
      await this.engine.refresh();
      this.toast.success(`Using ${m.display_name}`);
    } catch (e) {
      this.toast.error('Could not switch model', { description: messageFor(e) });
    }
  }
}
