import { ChangeDetectionStrategy, Component, computed, inject, input, OnInit, signal } from '@angular/core';
import { RouterLink } from '@angular/router';

import { ZardBadgeComponent } from '@/shared/components/badge/badge.component';
import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { ProviderModels } from '../../core/ipc/types';
import { EngineStore } from '../../core/stores/engine.store';
import { ModelsStore } from '../../core/stores/models.store';
import {
  localOptions,
  onlineOptions,
  type ProviderKind,
  providerOf,
  REMOTE_NOTICE_SETTING,
  type Role,
  type RoleOption,
  roleStatus,
} from './role-options';

const COPY: Record<Role, { title: string; subtitle: string }> = {
  'system-one': { title: 'System One', subtitle: 'Scores your library against each job.' },
  research: { title: 'Company research', subtitle: 'Researches each company on the web.' },
};

const STATUS = {
  ready: { label: 'Ready', type: 'secondary' },
  'needs-setup': { label: 'Needs setup', type: 'outline' },
  error: { label: 'Error', type: 'destructive' },
} as const;

/** One role: pick a provider, then a model from it. */
@Component({
  selector: 'app-role-card',
  imports: [RouterLink, ZardBadgeComponent, ZardButtonComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="grid gap-4 rounded-lg border p-4 text-sm">
      <div class="flex items-start justify-between gap-3">
        <div>
          <h2 class="font-semibold">{{ copy().title }}</h2>
          <p class="text-muted-foreground">{{ copy().subtitle }}</p>
        </div>
        <span data-testid="role-status"><z-badge [zType]="statusView().type">{{ statusView().label }}</z-badge></span>
      </div>

      @if (reason(); as r) {
        <p class="text-muted-foreground">{{ r }}</p>
      }

      @if (role() === 'research') {
        <div class="flex items-center gap-2">
          <span class="text-muted-foreground">Needs</span>
          @if (hasExa()) {
            <span>Exa key ✓</span>
          } @else {
            <a routerLink="/ai/providers" class="underline">Add your Exa key →</a>
          }
        </div>
      }

      <div role="group" aria-label="Provider" class="bg-muted text-muted-foreground inline-flex w-fit rounded-lg p-1">
        @for (p of providers(); track p.id) {
          <button
            type="button"
            class="rounded-md px-3 py-1 font-medium"
            [class]="viewing() === p.id ? 'bg-background text-foreground shadow-sm' : 'hover:text-foreground'"
            [attr.aria-pressed]="viewing() === p.id"
            (click)="view(p.id)"
          >
            {{ p.label }}
          </button>
        }
      </div>

      @if (pending(); as m) {
        <div class="grid gap-3 rounded-lg border p-4" role="alertdialog" aria-label="Online model notice">
          <p>Your CV entries and job descriptions will be sent to {{ keyName(providerOf(m.id)) }} and the model's provider.</p>
          <div class="flex gap-2">
            <button z-button zSize="sm" (click)="confirm(m)">Use {{ m.label }}</button>
            <button z-button zType="outline" zSize="sm" (click)="pending.set(null)">Cancel</button>
          </div>
        </div>
      }

      @if (viewing() !== 'local' && !hasKey(viewing())) {
        <div class="text-muted-foreground rounded-lg border border-dashed p-6 text-center">
          <a routerLink="/ai/providers" class="underline">Add your {{ keyName(viewing()) }} key →</a> to use online models.
        </div>
      } @else if (viewing() !== 'local' && onlineError(); as err) {
        <div class="grid justify-items-start gap-2 rounded-lg border p-4">
          <p class="text-destructive" role="alert">{{ err }}</p>
          <button z-button zType="outline" zSize="sm" [zLoading]="loading()" (click)="loadOnline(true)">Retry</button>
        </div>
      } @else if (!options().length) {
        <div class="text-muted-foreground rounded-lg border border-dashed p-6 text-center">
          @if (viewing() === 'local') {
            No model installed. <a routerLink="/ai/models" class="underline">Download one →</a>
          } @else {
            {{ loading() ? 'Loading online models…' : 'No online models available.' }}
          }
        </div>
      } @else {
        @if (viewing() !== 'local') {
          <div class="-mb-2 flex justify-end">
            <button z-button zType="ghost" zSize="sm" [zLoading]="loading()" (click)="loadOnline(true)">Refresh</button>
          </div>
        }
        <ul class="max-h-80 divide-y overflow-auto rounded-lg border" role="radiogroup" [attr.aria-label]="copy().title + ' model'">
          @for (o of options(); track o.id) {
            <li class="flex items-center gap-3 p-3" [class.opacity-60]="!o.installed">
              <button
                type="button"
                role="radio"
                class="flex flex-1 items-center gap-3 text-left disabled:cursor-not-allowed"
                [attr.data-id]="o.id"
                [attr.aria-checked]="o.id === active()"
                [disabled]="!o.installed"
                (click)="pick(o)"
              >
                <span
                  class="flex size-4 shrink-0 items-center justify-center rounded-full border"
                  [class.border-primary]="o.id === active()"
                  aria-hidden="true"
                >
                  @if (o.id === active()) {
                    <span class="bg-primary size-2 rounded-full"></span>
                  }
                </span>
                <span class="grid gap-0.5">
                  <span class="font-medium">{{ o.label }}</span>
                  @if (o.detail) {
                    <span class="text-muted-foreground text-xs">{{ o.detail }}</span>
                  }
                </span>
              </button>
              @if (!o.installed) {
                <a routerLink="/ai/models" class="text-xs underline">Download</a>
              } @else if (o.id === active()) {
                <span class="text-xs text-emerald-600">Active</span>
              }
            </li>
          }
        </ul>
      }

      @if (role() === 'system-one' && active()) {
        <div class="grid gap-2 border-t pt-3">
          <div class="flex flex-wrap items-center gap-3">
            <span class="text-muted-foreground">
              {{ engine.remote() ? 'Online model' : 'llama.cpp on 127.0.0.1, started on the first match' }} ·
              {{ engine.stateLabel() }}
            </span>
            <button z-button zType="outline" zSize="sm" (click)="restart()">Restart</button>
          </div>
          @if (engine.status().error; as log) {
            <details>
              <summary class="text-destructive cursor-pointer">Engine log</summary>
              <pre class="bg-muted mt-2 max-h-60 overflow-auto rounded p-2 text-xs" role="alert">{{ log }}</pre>
            </details>
          }
        </div>
      }
    </section>
  `,
})
export class RoleCardComponent implements OnInit {
  readonly role = input.required<Role>();
  protected readonly models = inject(ModelsStore);
  protected readonly engine = inject(EngineStore);
  private readonly toast = inject(ZardSonnerService);

  private readonly tabs: { id: ProviderKind; label: string; roles: Role[] }[] = [
    { id: 'local', label: 'Llama.cpp (local)', roles: ['system-one', 'research'] },
    { id: 'openrouter', label: 'OpenRouter (online)', roles: ['system-one', 'research'] },
    { id: 'subq', label: 'SubQ (online)', roles: ['system-one'] },
  ];
  protected readonly providerOf = providerOf;
  protected readonly providers = computed(() => this.tabs.filter((t) => t.roles.includes(this.role())));
  protected readonly copy = computed(() => COPY[this.role()]);
  /** A pick in flight; the store's value wins again once it settles. */
  private readonly picking = signal<string | null>(null);
  protected readonly active = computed(
    () => this.picking() ?? (this.role() === 'research' ? this.models.researchModel() : this.engine.status().model_id),
  );
  protected readonly viewing = signal<ProviderKind>('local');
  protected readonly hasOpenRouter = signal(false);
  protected readonly hasSubQ = signal(false);
  protected readonly hasExa = signal(false);
  protected readonly online = signal<ProviderModels[] | null>(null);
  protected readonly loading = signal(false);
  protected readonly pending = signal<RoleOption | null>(null);

  protected readonly onlineError = computed(
    () => this.online()?.find((p) => p.provider === this.viewing() && p.error)?.error ?? null,
  );
  protected readonly options = computed(() =>
    this.viewing() === 'local'
      ? localOptions(this.role(), this.models.models())
      : onlineOptions((this.online() ?? []).filter((p) => p.provider === this.viewing())),
  );
  private readonly status = computed(() =>
    roleStatus({
      active: this.active(),
      installed: this.models.installed(),
      hasOpenRouterKey: this.hasOpenRouter(),
      hasSubQKey: this.hasSubQ(),
      hasExaKey: this.role() === 'research' ? this.hasExa() : undefined,
      engineError: this.role() === 'system-one' && this.engine.status().state === 'error',
    }),
  );
  protected readonly statusView = computed(() => STATUS[this.status()]);
  protected readonly reason = computed(() => {
    if (this.status() !== 'needs-setup') return null;
    const a = this.active();
    if (!a) return 'Choose a model below.';
    if (providerOf(a) !== 'local' && !this.hasKey(providerOf(a))) return `The selected online model needs the ${this.keyName(providerOf(a))} key.`;
    if (providerOf(a) === 'local' && !this.models.installed().some((m) => m.spec.id === a)) return 'The selected model is not installed.';
    return null;
  });

  async ngOnInit(): Promise<void> {
    const research = this.role() === 'research';
    const [or, subq, exa] = await Promise.all([
      api.providerKeyStatus('openrouter').catch(() => null),
      api.providerKeyStatus('subq').catch(() => null),
      research ? api.providerKeyStatus('exa').catch(() => null) : null,
      research ? this.models.loadResearchModel() : null,
      this.models.load(),
      this.engine.load(),
    ]);
    this.hasOpenRouter.set(or?.has_key ?? false);
    this.hasSubQ.set(subq?.has_key ?? false);
    this.hasExa.set(exa?.has_key ?? false);
    this.view(providerOf(this.active()));
  }

  protected hasKey(p: ProviderKind): boolean {
    if (p === 'openrouter') return this.hasOpenRouter();
    if (p === 'subq') return this.hasSubQ();
    return true;
  }

  protected keyName(p: ProviderKind): string {
    return p === 'subq' ? 'SubQ' : 'OpenRouter';
  }

  protected view(p: ProviderKind): void {
    this.viewing.set(p);
    if (p !== 'local' && this.hasKey(p) && !this.online()) void this.loadOnline(false);
  }

  protected async loadOnline(refresh: boolean): Promise<void> {
    this.loading.set(true);
    try {
      this.online.set(await api.listRemoteModels(refresh, this.role() === 'research' ? 'chat' : 'decision'));
    } catch (e) {
      this.online.set([{ provider: this.viewing(), models: [], error: messageFor(e) }]);
    } finally {
      this.loading.set(false);
    }
  }

  protected async pick(o: RoleOption): Promise<void> {
    if (this.picking() || o.id === this.active() || !o.installed) return;
    if (providerOf(o.id) !== 'local') {
      const ack = await api.getUiSetting(REMOTE_NOTICE_SETTING).catch(() => null);
      if (ack !== '1') {
        this.pending.set(o);
        return;
      }
    }
    await this.select(o);
  }

  protected async confirm(o: RoleOption): Promise<void> {
    this.pending.set(null);
    await api.setUiSetting(REMOTE_NOTICE_SETTING, '1').catch(() => undefined);
    await this.select(o);
  }

  private async select(o: RoleOption): Promise<void> {
    this.picking.set(o.id);
    try {
      if (this.role() === 'research') await this.models.useForResearch(o.id);
      else {
        await this.models.activate(o.id);
        await this.engine.refresh();
      }
    } catch (err) {
      this.toast.error('Could not switch model', { description: messageFor(err) });
    } finally {
      this.picking.set(null);
    }
  }

  /** Re-selecting the active model resets the engine (and its error). */
  protected async restart(): Promise<void> {
    const id = this.active();
    if (!id) return;
    try {
      await this.models.activate(id);
      await this.engine.refresh();
      this.toast.success('Engine restarted', { description: 'It starts again on the next match.' });
    } catch (e) {
      this.toast.error('Could not restart', { description: messageFor(e) });
    }
  }
}
