import { ChangeDetectionStrategy, Component, computed, inject, input, OnInit, signal } from '@angular/core';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { KeyStatus, ProviderId } from '../../core/ipc/types';

const COPY: Record<ProviderId, { name: string; placeholder: string; hint: string }> = {
  openrouter: {
    name: 'OpenRouter',
    placeholder: 'sk-or-...',
    hint: 'Lets you run online models. Matching or researching with them sends your CV and job text to OpenRouter.',
  },
  subq: {
    name: 'SubQ',
    placeholder: 'SubQ API key',
    hint: 'Lets you run SubQ Snap for decisions. Matching with it sends your CV and job text to SubQ.',
  },
  exa: {
    name: 'Exa',
    placeholder: 'Exa API key',
    hint: 'Web search for company research. Search queries (company names) are sent to Exa.',
  },
};

/** A provider API key: paste, validate, save, remove. The key itself never comes back from the backend. */
@Component({
  selector: 'app-provider-key',
  imports: [ZardButtonComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="grid gap-2 text-sm">
      <span class="font-medium" data-testid="provider-title">{{ copy().name }}</span>
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
            [placeholder]="copy().placeholder"
            [attr.aria-label]="copy().name + ' API key'"
            class="border-input h-8 flex-1 rounded-md border bg-transparent px-2"
            [value]="draft()"
            (input)="draft.set($any($event.target).value)"
          />
          <button z-button zSize="sm" [zLoading]="busy()" [zDisabled]="!draft().trim()" (click)="save()">Save</button>
        </div>
        <p class="text-muted-foreground">{{ copy().hint }}</p>
      }
    </div>
  `,
})
export class ProviderKeyComponent implements OnInit {
  readonly provider = input<ProviderId>('openrouter');
  protected readonly copy = computed(() => COPY[this.provider()]);
  private readonly toast = inject(ZardSonnerService);
  protected readonly status = signal<KeyStatus>({ has_key: false, last4: null });
  protected readonly draft = signal('');
  protected readonly busy = signal(false);

  ngOnInit(): void {
    void api.providerKeyStatus(this.provider()).then((s) => this.status.set(s), () => undefined);
  }

  async save(): Promise<void> {
    this.busy.set(true);
    try {
      this.status.set(await api.setProviderKey(this.provider(), this.draft()));
      this.draft.set('');
      this.toast.success(`${this.copy().name} key saved`);
    } catch (e) {
      this.toast.error('Could not save key', { description: messageFor(e) });
    } finally {
      this.busy.set(false);
    }
  }

  async remove(): Promise<void> {
    this.busy.set(true);
    try {
      await api.deleteProviderKey(this.provider());
      this.status.set({ has_key: false, last4: null });
    } catch (e) {
      this.toast.error('Could not remove key', { description: messageFor(e) });
    } finally {
      this.busy.set(false);
    }
  }
}
