import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { messageFor } from '../../core/ipc/errors';
import type { ProviderId } from '../../core/ipc/types';
import { ConnectionsStore } from '../../core/stores/connections.store';
import { PROVIDERS } from './provider-registry';

/** One online provider and its saved keys, with a form to add another. */
@Component({
  selector: 'app-provider-connections',
  imports: [ZardButtonComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid gap-3 text-sm">
      <span class="font-medium" data-testid="provider-title">{{ def().name }}</span>

      @for (c of connections(); track c.id) {
        <div class="flex flex-wrap items-center gap-3" [attr.data-connection]="c.id">
          <span class="font-medium">{{ c.label }}</span>
          <span class="text-muted-foreground" data-testid="key-saved">Key saved (…{{ c.last4 }})</span>
          <button z-button zType="outline" zSize="sm" [zLoading]="busy() === c.id" (click)="remove(c.id)">Remove</button>
        </div>
      }

      <div class="flex max-w-xl flex-wrap gap-2">
        <input
          type="text"
          autocomplete="off"
          placeholder="Name, e.g. Work"
          [attr.aria-label]="def().name + ' key name'"
          class="border-input h-8 w-36 rounded-md border bg-transparent px-2"
          [value]="label()"
          (input)="label.set($any($event.target).value)"
        />
        <input
          type="password"
          autocomplete="off"
          [placeholder]="def().placeholder"
          [attr.aria-label]="def().name + ' API key'"
          class="border-input h-8 min-w-48 flex-1 rounded-md border bg-transparent px-2"
          [value]="draft()"
          (input)="draft.set($any($event.target).value)"
        />
        <button z-button zSize="sm" [zLoading]="busy() === 'add'" [zDisabled]="!draft().trim()" (click)="add()">
          {{ connections().length ? 'Add another key' : 'Save' }}
        </button>
      </div>
      <p class="text-muted-foreground">{{ def().hint }}</p>
    </div>
  `,
})
export class ProviderConnectionsComponent {
  readonly provider = input.required<ProviderId>();
  private readonly store = inject(ConnectionsStore);
  private readonly toast = inject(ZardSonnerService);

  protected readonly def = computed(() => PROVIDERS.find((p) => p.id === this.provider()) ?? PROVIDERS[0]);
  protected readonly connections = computed(() => this.store.forProvider(this.provider()));
  protected readonly label = signal('');
  protected readonly draft = signal('');
  protected readonly busy = signal<string | null>(null);

  protected async add(): Promise<void> {
    this.busy.set('add');
    try {
      // Unnamed keys take the provider's name, or "Key 2" when it already has one.
      const n = this.connections().length;
      const label = this.label().trim() || (n ? `Key ${n + 1}` : this.def().name);
      await this.store.add(this.provider(), label, this.draft());
      this.label.set('');
      this.draft.set('');
      this.toast.success(`${this.def().name} key saved`);
    } catch (e) {
      this.toast.error('Could not save key', { description: messageFor(e) });
    } finally {
      this.busy.set(null);
    }
  }

  protected async remove(id: string): Promise<void> {
    this.busy.set(id);
    try {
      await this.store.remove(id);
    } catch (e) {
      this.toast.error('Could not remove key', { description: messageFor(e) });
    } finally {
      this.busy.set(null);
    }
  }
}
