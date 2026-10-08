import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { KeyStatus } from '../../core/ipc/types';

/** OpenRouter API key: paste, validate, save, remove. The key itself never comes back from the backend. */
@Component({
  selector: 'app-provider-key',
  imports: [ZardButtonComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid gap-2 text-sm">
      <span class="font-medium">OpenRouter</span>
      @if (status().has_key) {
        <div class="flex items-center gap-3">
          <span data-testid="key-saved">Key saved (…{{ status().last4 }})</span>
          <button z-button zType="outline" zSize="sm" [zLoading]="busy()" (click)="remove()">Remove</button>
        </div>
      } @else {
        <div class="flex max-w-md gap-2">
          <input
            type="password"
            autocomplete="off"
            placeholder="sk-or-..."
            aria-label="OpenRouter API key"
            class="border-input h-8 flex-1 rounded-md border bg-transparent px-2"
            [value]="draft()"
            (input)="draft.set($any($event.target).value)"
          />
          <button z-button zSize="sm" [zLoading]="busy()" [zDisabled]="!draft().trim()" (click)="save()">Save</button>
        </div>
        <p class="text-muted-foreground">Lets you run online decision models. Matching with them sends your CV and job text to OpenRouter.</p>
      }
    </div>
  `,
})
export class ProviderKeyComponent implements OnInit {
  private readonly toast = inject(ZardSonnerService);
  protected readonly status = signal<KeyStatus>({ has_key: false, last4: null });
  protected readonly draft = signal('');
  protected readonly busy = signal(false);

  ngOnInit(): void {
    void api.providerKeyStatus('openrouter').then((s) => this.status.set(s), () => undefined);
  }

  async save(): Promise<void> {
    this.busy.set(true);
    try {
      this.status.set(await api.setProviderKey('openrouter', this.draft()));
      this.draft.set('');
      this.toast.success('OpenRouter key saved');
    } catch (e) {
      this.toast.error('Could not save key', { description: messageFor(e) });
    } finally {
      this.busy.set(false);
    }
  }

  async remove(): Promise<void> {
    this.busy.set(true);
    try {
      await api.deleteProviderKey('openrouter');
      this.status.set({ has_key: false, last4: null });
    } catch (e) {
      this.toast.error('Could not remove key', { description: messageFor(e) });
    } finally {
      this.busy.set(false);
    }
  }
}
