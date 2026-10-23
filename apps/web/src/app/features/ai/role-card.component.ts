import { platform } from '../../core/platform/runtime';
import { ChangeDetectionStrategy, Component, computed, effect, inject, input, OnInit, signal, untracked } from '@angular/core';

import { ZardBadgeComponent } from '@/shared/components/badge/badge.component';
import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { ProviderModels } from '../../core/ipc/types';
import { ConnectionsStore } from '../../core/stores/connections.store';
import { EngineStore } from '../../core/stores/engine.store';
import { ModelsStore } from '../../core/stores/models.store';
import { ProviderKeyComponent } from './provider-key.component';
import { modelProviders, providerName, type Role } from './provider-registry';
import { onlineOptions, providerOf, REMOTE_NOTICE_SETTING, type RoleOption, roleStatus } from './role-options';

const COPY: Record<Role, { title: string; subtitle: string }> = {
  'system-one': { title: 'Fast Model', subtitle: 'Scores your library against each job and sorts the results.' },
  research: { title: 'Smart Model', subtitle: 'Researches companies on the web and writes summaries.' },
};

const STATUS = {
  ready: { label: 'Ready', type: 'secondary' },
  'needs-setup': { label: 'Needs setup', type: 'outline' },
  error: { label: 'Error', type: 'destructive' },
} as const;

interface Group {
  /** Connection id (what model refs carry), or the provider id while it has no key. */
  id: string;
  provider: string;
  name: string;
  options: RoleOption[];
  /** Needs a key first. */
  locked: boolean;
  error: string | null;
}

/** One model slot: shows the active model and a grouped picker over every connected provider. */
@Component({
  selector: 'app-role-card',
  imports: [ ZardBadgeComponent, ZardButtonComponent, ProviderKeyComponent],
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

      @if (role() === 'research' && !hasExa()) {
        <div class="grid gap-2 rounded-lg border border-dashed p-3">
          <p class="text-muted-foreground">Company research needs an Exa key for web search.</p>
          <app-provider-key provider="exa" (changed)="refreshExa()" />
        </div>
      }

      <div class="flex items-center justify-between gap-3 rounded-lg border p-3" data-testid="active-model">
        <div class="grid gap-0.5">
          <span class="font-medium">{{ activeLabel() }}</span>
          <span class="text-muted-foreground text-xs">{{ activeWhere() }}</span>
        </div>
        <button z-button zType="outline" zSize="sm" [attr.aria-expanded]="open()" (click)="open.set(!open())">
          {{ open() ? 'Close' : 'Change' }}
        </button>
      </div>

      @if (pending(); as m) {
        <div class="grid gap-3 rounded-lg border p-4" role="alertdialog" aria-label="Online model notice">
          <p>Your CV entries and job descriptions will be sent to {{ providerName(providerOf(m.id)) }} and the model's provider.</p>
          <div class="flex gap-2">
            <button z-button zSize="sm" (click)="confirm(m)">Use {{ m.label }}</button>
            <button z-button zType="outline" zSize="sm" (click)="pending.set(null)">Cancel</button>
          </div>
        </div>
      }

      @if (open()) {
        <div class="grid gap-3" data-testid="picker">
          <div class="flex gap-2">
            <input
              type="search"
              placeholder="Search models"
              aria-label="Search models"
              class="border-input h-8 flex-1 rounded-md border bg-transparent px-2"
              [value]="query()"
              (input)="query.set($any($event.target).value)"
            />
            @if (anyOnline()) {
              <button z-button zType="ghost" zSize="sm" [zLoading]="loading()" (click)="loadOnline(true)">Refresh</button>
            }
          </div>

          <div class="max-h-96 divide-y overflow-auto rounded-lg border" role="radiogroup" [attr.aria-label]="copy().title + ' model'">
            @for (g of groups(); track g.id) {
              <div class="grid" [attr.data-group]="g.id">
                <div class="bg-muted/50 text-muted-foreground px-3 py-1.5 text-xs font-medium">{{ g.name }}</div>
                @if (g.locked) {
                  <div class="grid gap-2 p-3">
                    @if (g.provider === 'localhost') {
                      <p class="text-muted-foreground">Set up your Local server connection below.</p>
                    } @else {
                      <p class="text-muted-foreground">Add your {{ g.name }} key to use its models.</p>
                      <app-provider-key [provider]="$any(g.provider)" (changed)="connections.load()" />
                    }
                  </div>
                } @else if (g.error; as err) {
                  <div class="grid justify-items-start gap-2 p-3">
                    <p class="text-destructive" role="alert">{{ err }}</p>
                    <button z-button zType="outline" zSize="sm" [zLoading]="loading()" (click)="loadOnline(true)">Retry</button>
                  </div>
                } @else if (!g.options.length) {
                  <p class="text-muted-foreground p-3">
                    {{ loading() ? 'Loading models…' : query() ? 'No matching model.' : 'No models available.' }}
                  </p>
                } @else {
                  @for (o of g.options; track o.id) {
                    <div class="flex items-center gap-3 p-3" [class.opacity-60]="!o.installed">
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
                      @if (o.id === active()) {
                        <span class="text-xs text-emerald-600">Active</span>
                      }
                    </div>
                  }
                }
              </div>
            }
          </div>
        </div>
      }

      @if (role() === 'system-one' && active()) {
        <div class="grid gap-2 border-t pt-3">
          <div class="flex flex-wrap items-center gap-3">
            <span class="text-muted-foreground">
              {{ engine.remote() ? 'Online model' : 'User-run local server' }} ·
              {{ engine.stateLabel() }}
            </span>
            <button z-button zType="outline" zSize="sm" (click)="restart()">Reset connection</button>
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
  protected readonly connections = inject(ConnectionsStore);
  private readonly toast = inject(ZardSonnerService);

  protected readonly providerOf = providerOf;
  protected readonly providerName = providerName;
  protected readonly copy = computed(() => COPY[this.role()]);
  /** A pick in flight; the store's value wins again once it settles. */
  private readonly picking = signal<string | null>(null);
  protected readonly active = computed(
    () => this.picking() ?? (this.role() === 'research' ? this.models.researchModel() : this.engine.status().model_id),
  );

  private readonly exa = signal(false);
  /** Saved keys by connection id (includes Exa for research). */
  protected readonly keys = computed<Record<string, boolean>>(() => ({
    ...Object.fromEntries([...this.connections.ids()].map((id) => [id, true])),
    exa: this.exa(),
  }));
  protected readonly hasExa = computed(() => this.role() !== 'research' || this.exa());
  protected readonly online = signal<ProviderModels[] | null>(null);
  protected readonly loading = signal(false);
  protected readonly pending = signal<RoleOption | null>(null);
  protected readonly open = signal(false);
  protected readonly query = signal('');
  /** This slot's connections, in provider order. */
  private readonly slotConnections = computed(() => {
    const kinds = modelProviders(this.role()).map((p) => p.id as string);
    return this.connections.list().filter((c) => kinds.includes(c.provider));
  });
  protected readonly anyOnline = computed(() => this.slotConnections().length > 0);
  /** Changes when a key is added or removed, so the model lists reload. */
  private readonly connectionKey = computed(() => `${this.connections.revision()}:${this.slotConnections().map((c) => c.id).join(',')}`);

  protected readonly groups = computed<Group[]>(() => {
    const q = this.query().trim().toLowerCase();
    const match = (o: RoleOption) => !q || o.label.toLowerCase().includes(q);
    const remote = modelProviders(this.role()).filter((p) => platform.capabilities.localServer || p.id !== 'localhost').flatMap<Group>((p) => {
      const mine = this.connections.forProvider(p.id);
      if (!mine.length) return [{ id: p.id, provider: p.id, name: p.name, options: [], locked: true, error: null }];
      return mine.map<Group>((c) => {
        const list = this.online()?.find((l) => l.provider === c.id);
        return {
          id: c.id,
          provider: p.id,
          // "OpenRouter" alone, or "OpenRouter · Work" once the key has its own name.
          name: c.label === p.name ? p.name : `${p.name} · ${c.label}`,
          options: onlineOptions(list ? [list] : []).filter(match),
          locked: false,
          error: list?.error ?? null,
        };
      });
    });
    return remote;
  });

  private readonly status = computed(() =>
    roleStatus({
      active: this.active(),
      keys: this.keys(),
      hasExaKey: this.role() === 'research' ? this.hasExa() : undefined,
      engineError: this.role() === 'system-one' && this.engine.status().state === 'error',
    }),
  );
  protected readonly statusView = computed(() => STATUS[this.status()]);
  protected readonly reason = computed(() => {
    if (this.status() !== 'needs-setup') return null;
    const a = this.active();
    if (!a) return 'Choose a model below.';
    const p = providerOf(a);
    if (!this.keys()[p]) return `Set up the ${providerName(p)} connection for the selected model.`;
    return null;
  });

  protected readonly activeLabel = computed(() => {
    const a = this.active();
    if (!a) return 'No model selected';
    const hit = this.groups()
      .flatMap((g) => g.options)
      .find((o) => o.id === a);
    return hit?.label ?? a.slice(a.indexOf(':') + 1);
  });
  protected readonly activeWhere = computed(() => {
    const a = this.active();
    if (!a) return 'Pick one to get started.';
    const p = providerOf(a);
    if (p === 'localhost') return 'User-run local server';
    const label = this.connections.list().find((c) => c.id === p)?.label;
    return label && label !== providerName(p) ? `Online · ${providerName(p)} · ${label}` : `Online · ${providerName(p)}`;
  });

  constructor() {
    // Model lists follow the saved keys, whichever component changed them.
    let seen: string | null = null;
    effect(() => {
      const key = this.connectionKey();
      untracked(() => {
        if (seen !== null && key !== seen) void this.syncOnline();
        seen = key;
      });
    });
  }

  async ngOnInit(): Promise<void> {
    const research = this.role() === 'research';
    await Promise.all([
      research ? this.models.loadResearchModel() : null,
      this.engine.load(),
      this.connections.load(),
      research ? this.refreshExa() : null,
    ]);
    this.open.set(!this.active());
    // Keys present on load produce the first list; later changes go through the effect.
    if (this.anyOnline() && !this.online()) await this.loadOnline(false);
  }

  protected async refreshExa(): Promise<void> {
    this.exa.set((await api.providerKeyStatus('exa').catch(() => null))?.has_key ?? false);
  }

  private async syncOnline(): Promise<void> {
    if (this.anyOnline()) await this.loadOnline(false);
    else this.online.set(null);
  }

  protected async loadOnline(refresh: boolean): Promise<void> {
    this.loading.set(true);
    try {
      this.online.set(await api.listRemoteModels(refresh, this.role() === 'research' ? 'chat' : 'decision'));
    } catch (e) {
      const message = messageFor(e);
      this.online.set(this.slotConnections().map((c) => ({ provider: c.id, label: c.label, models: [], error: message })));
    } finally {
      this.loading.set(false);
    }
  }

  protected async pick(o: RoleOption): Promise<void> {
    if (this.picking() || o.id === this.active() || !o.installed) return;
    if (providerOf(o.id) !== 'localhost') {
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

  /** Re-selecting the active model resets the cached HTTP client and error. */
  protected async restart(): Promise<void> {
    const id = this.active();
    if (!id) return;
    try {
      await this.models.activate(id);
      await this.engine.refresh();
      this.toast.success('Connection reset', { description: 'CVD reconnects on the next match.' });
    } catch (e) {
      this.toast.error('Could not restart', { description: messageFor(e) });
    }
  }
}
