import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';
import { ZardButtonComponent } from '@/shared/components/button/button.component';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import { ConnectionsStore } from '../../core/stores/connections.store';
import type { RemoteModel } from '../../core/ipc/types';

@Component({
  selector: 'app-local-provider',
  imports: [ZardButtonComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid gap-3 text-sm" data-testid="local-provider">
      <span class="font-medium" data-testid="provider-title">Local server</span>
      <p class="text-muted-foreground">Install and start llama.cpp yourself, then connect here. CVD does not install models or start servers.</p>
      <label class="grid gap-1">Base URL
        <input class="rounded-md border bg-transparent px-2 py-1" aria-label="Local server base URL" [value]="url()" (input)="url.set($any($event.target).value)" />
      </label>
      <label class="grid gap-1">API key (optional)
        <input type="password" autocomplete="off" class="rounded-md border bg-transparent px-2 py-1" aria-label="Local server API key" [placeholder]="hasKey() ? 'Key saved; leave blank to keep it' : 'No key required'" [value]="key()" (input)="key.set($any($event.target).value)" />
      </label>
      @if (hasKey()) {
        <label><input type="checkbox" [checked]="clearKey()" (change)="clearKey.set($any($event.target).checked)" /> Remove saved API key</label>
      }
      <div class="flex gap-2">
        <button z-button zSize="sm" [zLoading]="busy()" (click)="save()">Save and discover models</button>
        @if (connected()) { <button z-button zSize="sm" zType="outline" [zDisabled]="busy()" (click)="remove()">Remove</button> }
      </div>
      @if (available().length) {
        <label class="grid gap-1">Model to test
          <select aria-label="Local model to test" class="rounded-md border bg-background p-1" [value]="model()" (change)="model.set($any($event.target).value)">
            @for (m of available(); track m.id) { <option [value]="m.id">{{ m.display_name }}</option> }
          </select>
        </label>
        <div class="flex gap-2">
          <button z-button zSize="sm" zType="outline" [zDisabled]="busy()" (click)="test('decision')">Test Fast Model</button>
          <button z-button zSize="sm" zType="outline" [zDisabled]="busy()" (click)="test('chat')">Test Smart Model</button>
        </div>
      }
      @if (error()) { <p class="text-destructive" role="alert">{{ error() }}</p> }
      @if (result()) { <p role="status">{{ result() }}</p> }
      <details>
        <summary class="cursor-pointer">How to run llama-server</summary>
        <p class="mt-2">Install llama.cpp from ggml-org/llama.cpp releases or build it from source. Download a compatible GGUF separately.</p>
        <p class="mt-2">Fast Model requires a System One model and a server exposing /v1/systemone, with logprobs/grammar support. Chat-only OpenAI-compatible servers can serve the Smart Model.</p>
        <pre class="mt-2 overflow-auto rounded bg-muted p-2 text-xs">llama-server -m /path/to/decision.gguf --host 127.0.0.1 --port 8080 -b 16384 -ub 16384</pre>
        <p class="mt-2">For company research, use a tool-capable chat model and add --jinja -c 32768. To run both at once, serve both models behind one compatible endpoint. Company research also needs an Exa key.</p>
      </details>
    </div>
  `,
})
export class LocalProviderComponent implements OnInit {
  private readonly connections = inject(ConnectionsStore);
  protected readonly url = signal('http://127.0.0.1:8080/v1');
  protected readonly key = signal('');
  protected readonly hasKey = signal(false);
  protected readonly clearKey = signal(false);
  protected readonly connected = signal(false);
  protected readonly available = signal<RemoteModel[]>([]);
  protected readonly model = signal('');
  protected readonly busy = signal(false);
  protected readonly error = signal<string | null>(null);
  protected readonly result = signal<string | null>(null);

  async ngOnInit(): Promise<void> {
    try {
      const c = await api.getLocalConnection();
      this.url.set(c.base_url); this.hasKey.set(c.has_key);
      await this.connections.load();
      this.connected.set(this.connections.ids().has('localhost'));
      if (this.connected()) await this.discover();
    } catch (e) { this.error.set(messageFor(e)); }
  }
  private async discover(): Promise<void> {
    const list = (await api.listRemoteModels(true, 'decision')).find(p => p.provider === 'localhost');
    this.available.set(list?.models ?? []);
    this.model.set(list?.models[0]?.id ?? '');
    if (list?.error) this.error.set(list.error);
  }
  protected async save(): Promise<void> {
    this.busy.set(true); this.error.set(null); this.result.set(null);
    try {
      await api.saveLocalConnection(this.url(), this.clearKey() ? '' : this.key().trim() || null);
      this.hasKey.set(!this.clearKey() && (!!this.key().trim() || this.hasKey()));
      this.key.set(''); this.clearKey.set(false); this.connected.set(true);
      await this.connections.load(); await this.discover();
      this.result.set('Connection saved. Choose a model for each role above; test its capabilities here.');
    } catch (e) { this.error.set(messageFor(e)); }
    finally { this.busy.set(false); }
  }
  protected async test(role: 'decision' | 'chat'): Promise<void> {
    this.busy.set(true); this.error.set(null); this.result.set(null);
    try { await api.testLocalConnection(this.model(), role); this.result.set(`${role === 'decision' ? 'Fast' : 'Smart'} Model test passed.`); }
    catch (e) { this.error.set(messageFor(e)); }
    finally { this.busy.set(false); }
  }
  protected async remove(): Promise<void> {
    this.busy.set(true); this.error.set(null); this.result.set(null);
    try { await this.connections.remove('localhost'); this.connected.set(false); this.available.set([]); this.hasKey.set(false); }
    catch (e) { this.error.set(messageFor(e)); }
    finally { this.busy.set(false); }
  }
}
