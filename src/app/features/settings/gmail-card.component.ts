import { ChangeDetectionStrategy, Component, computed, inject, OnInit, signal } from '@angular/core';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { MailAccountView } from '../../core/ipc/types';

const DEFAULT_HOST = 'imap.gmail.com';
const DEFAULT_PORT = 993;
const DEFAULT_FOLDER = '[Gmail]/All Mail';

/** Gmail over IMAP: address and app password, used to read job update emails. */
@Component({
  selector: 'app-gmail-card',
  imports: [ZardButtonComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <section class="grid gap-3 rounded-xl border p-4 text-sm" aria-labelledby="gmail-title">
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 id="gmail-title" class="font-medium">Gmail</h3>
          <p class="text-muted-foreground">Reads the last 30 days of mail to find job application updates.</p>
        </div>
        @if (account(); as a) {
          <span
            data-testid="gmail-status"
            class="rounded px-1.5 py-0.5 text-xs leading-4 font-medium bg-emerald-100 text-emerald-800 dark:bg-emerald-900/50 dark:text-emerald-200"
            >Connected as {{ a.address }}</span
          >
        } @else {
          <span data-testid="gmail-status" class="text-muted-foreground">Not connected</span>
        }
      </div>

      <div class="grid max-w-md grid-cols-[1fr_6rem] gap-2">
        <label class="grid gap-1.5">
          <span class="font-medium">IMAP host</span>
          <input
            class="border-input h-8 rounded-md border bg-transparent px-2"
            aria-label="IMAP host"
            [value]="host()"
            (input)="host.set($any($event.target).value)"
          />
        </label>
        <label class="grid gap-1.5">
          <span class="font-medium">Port</span>
          <input
            type="number"
            class="border-input h-8 rounded-md border bg-transparent px-2"
            aria-label="IMAP port"
            [value]="port()"
            (input)="port.set(+$any($event.target).value)"
          />
        </label>
      </div>

      <label class="grid max-w-md gap-1.5">
        <span class="font-medium">Gmail address</span>
        <input
          type="email"
          autocomplete="off"
          class="border-input h-8 rounded-md border bg-transparent px-2"
          aria-label="Gmail address"
          [value]="address()"
          (input)="address.set($any($event.target).value)"
        />
      </label>

      <label class="grid max-w-md gap-1.5">
        <span class="font-medium">Mailbox to scan</span>
        <select
          class="border-input h-8 rounded-md border bg-transparent px-2"
          aria-label="Mailbox to scan"
          (change)="folder.set($any($event.target).value)"
        >
          @for (f of folderOptions(); track f) {
            <option [value]="f" [selected]="f === folder()">{{ f }}</option>
          }
        </select>
        <span class="text-muted-foreground"
          >Gmail's All Mail holds every message once. Test the connection to load your mailboxes.</span
        >
      </label>

      <div class="grid max-w-md gap-1.5">
        <span class="font-medium">App password</span>
        <div class="flex gap-2">
          <input
            [type]="showPassword() ? 'text' : 'password'"
            autocomplete="off"
            [placeholder]="account() ? 'Saved — leave empty to keep it' : 'xxxx xxxx xxxx xxxx'"
            class="border-input h-8 flex-1 rounded-md border bg-transparent px-2"
            aria-label="App password"
            [value]="password()"
            (input)="password.set($any($event.target).value)"
          />
          <button z-button zType="outline" zSize="sm" type="button" (click)="showPassword.set(!showPassword())">
            {{ showPassword() ? 'Hide' : 'Show' }}
          </button>
        </div>
        <p class="text-muted-foreground">
          Create an app password at myaccount.google.com/apppasswords (needs 2-Step Verification). It is stored on this device.
        </p>
      </div>

      <div class="flex flex-wrap gap-2">
        <button z-button zType="outline" zSize="sm" [zLoading]="busy()" (click)="test()">Test connection</button>
        <button z-button zSize="sm" [zLoading]="busy()" (click)="save()">Save</button>
        @if (account()) {
          <button z-button zType="outline" zSize="sm" [zLoading]="busy()" (click)="remove()">Remove</button>
        }
      </div>
    </section>
  `,
})
export class GmailCardComponent implements OnInit {
  private readonly toast = inject(ZardSonnerService);
  protected readonly account = signal<MailAccountView | null>(null);
  protected readonly host = signal(DEFAULT_HOST);
  protected readonly port = signal(DEFAULT_PORT);
  protected readonly address = signal('');
  protected readonly folder = signal(DEFAULT_FOLDER);
  protected readonly folders = signal<string[]>([]);
  /** The saved mailbox stays selectable even before the list loads. */
  protected readonly folderOptions = computed(() => [...new Set([this.folder(), ...this.folders()])]);
  protected readonly password = signal('');
  protected readonly showPassword = signal(false);
  protected readonly busy = signal(false);

  ngOnInit(): void {
    void api.getMailAccount().then((a) => {
      this.load(a);
      if (a) void this.refreshFolders(false);
    }, () => undefined);
  }

  /** An empty password field sends `null`, which keeps the stored one. */
  private passwordArg(): string | null {
    return this.password() || null;
  }

  private load(a: MailAccountView | null): void {
    this.account.set(a);
    if (!a) return;
    this.host.set(a.host);
    this.port.set(a.port);
    this.address.set(a.address);
    this.folder.set(a.folder);
  }

  /** Loads the mailboxes from the typed values. `toast` surfaces failures. */
  private async refreshFolders(toast: boolean): Promise<void> {
    try {
      this.folders.set(await api.listMailFolders(this.host(), this.port(), this.address(), this.passwordArg()));
    } catch (e) {
      if (toast) this.toast.error('Could not load mailboxes', { description: messageFor(e) });
    }
  }

  protected async test(): Promise<void> {
    this.busy.set(true);
    try {
      await api.testMailAccount(this.host(), this.port(), this.address(), this.passwordArg());
      this.toast.success('Gmail connection works');
      await this.refreshFolders(true);
    } catch (e) {
      this.toast.error('Could not connect to Gmail', { description: messageFor(e) });
    } finally {
      this.busy.set(false);
    }
  }

  protected async save(): Promise<void> {
    this.busy.set(true);
    try {
      const saved = await api.saveMailAccount(this.host(), this.port(), this.address(), this.passwordArg(), this.folder());
      this.account.set(saved);
      this.password.set('');
      this.toast.success('Gmail connected');
    } catch (e) {
      this.toast.error('Could not save Gmail settings', { description: messageFor(e) });
    } finally {
      this.busy.set(false);
    }
  }

  protected async remove(): Promise<void> {
    if (!window.confirm('Remove the Gmail connection? Job updates will stop until you connect again.')) return;
    this.busy.set(true);
    try {
      await api.deleteMailAccount();
      this.account.set(null);
      this.password.set('');
    } catch (e) {
      this.toast.error('Could not remove Gmail', { description: messageFor(e) });
    } finally {
      this.busy.set(false);
    }
  }
}
