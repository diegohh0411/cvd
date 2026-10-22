import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { MailAccountView } from '../../core/ipc/types';

const DEFAULT_HOST = 'imap.gmail.com';
const DEFAULT_PORT = 993;
const DEFAULT_FOLDER = '[Gmail]/All Mail';

/**
 * Gmail over IMAP. Steps: enter the address and app password, test the
 * connection (which lists the mailboxes), choose one, then save.
 */
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

      <details class="max-w-md">
        <summary class="text-muted-foreground cursor-pointer">Server settings</summary>
        <div class="mt-2 grid grid-cols-[1fr_6rem] gap-2">
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
      </details>

      <div class="flex flex-wrap gap-2">
        <button z-button zType="outline" zSize="sm" [zLoading]="busy()" (click)="test()">Test connection</button>
      </div>

      @if (folders().length) {
        <label class="grid max-w-md gap-1.5">
          <span class="font-medium">Mailbox to scan</span>
          <select
            class="border-input h-8 rounded-md border bg-transparent px-2"
            aria-label="Mailbox to scan"
            (change)="folder.set($any($event.target).value)"
          >
            <option value="" [selected]="!folder()" disabled>Choose a mailbox</option>
            @for (f of folders(); track f) {
              <option [value]="f" [selected]="f === folder()">{{ f }}</option>
            }
          </select>
          <span class="text-muted-foreground"
            >Gmail's All Mail holds every message once. Pick the mailbox where your job emails arrive.</span
          >
        </label>

        <div class="flex flex-wrap gap-2">
          <button z-button zSize="sm" [zLoading]="busy()" [zDisabled]="!folder()" (click)="save()">Save</button>
          @if (account()) {
            <button z-button zType="outline" zSize="sm" [zLoading]="busy()" (click)="remove()">Remove</button>
          }
        </div>
      } @else if (account()) {
        <div class="flex flex-wrap gap-2">
          <button z-button zType="outline" zSize="sm" [zLoading]="busy()" (click)="remove()">Remove</button>
        </div>
      }
    </section>
  `,
})
export class GmailCardComponent implements OnInit {
  private readonly toast = inject(ZardSonnerService);
  protected readonly account = signal<MailAccountView | null>(null);
  protected readonly host = signal(DEFAULT_HOST);
  protected readonly port = signal(DEFAULT_PORT);
  protected readonly address = signal('');
  protected readonly password = signal('');
  protected readonly showPassword = signal(false);
  /** Mailboxes from the last successful test; empty until the test succeeds. */
  protected readonly folders = signal<string[]>([]);
  /** The chosen mailbox; empty until one is picked. */
  protected readonly folder = signal('');
  protected readonly busy = signal(false);

  ngOnInit(): void {
    void api.getMailAccount().then((a) => {
      this.load(a);
      if (a) void this.loadMailboxes(false);
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

  /** Lists the mailboxes; keeps the chosen one only if the account still has it. */
  private async loadMailboxes(toast: boolean): Promise<void> {
    try {
      const list = await api.testMailAccount(this.host(), this.port(), this.address(), this.passwordArg());
      this.folders.set(list);
      if (!list.includes(this.folder())) this.folder.set(list.includes(DEFAULT_FOLDER) ? DEFAULT_FOLDER : '');
    } catch (e) {
      if (toast) this.toast.error('Could not connect to Gmail', { description: messageFor(e) });
    }
  }

  protected async test(): Promise<void> {
    this.busy.set(true);
    try {
      await this.loadMailboxes(true);
      if (this.folders().length) this.toast.success('Connected. Choose a mailbox to scan.');
    } finally {
      this.busy.set(false);
    }
  }

  protected async save(): Promise<void> {
    if (!this.folder()) return;
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
      this.folders.set([]);
      this.folder.set('');
    } catch (e) {
      this.toast.error('Could not remove Gmail', { description: messageFor(e) });
    } finally {
      this.busy.set(false);
    }
  }
}
