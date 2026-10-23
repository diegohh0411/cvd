import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';

import { platform } from '../../core/platform/runtime';

import { type DarkModeOptions, ZardDarkMode } from '@/shared/services/dark-mode';
import * as api from '../../core/ipc/commands';
import { PageHeaderComponent } from '../../shell/page-header.component';
import { BUDGET_SETTING } from '../workspace/workspace.store';
import { GmailCardComponent } from './gmail-card.component';

@Component({
  selector: 'app-settings-page',
  imports: [PageHeaderComponent, GmailCardComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'Settings' }]" />
    <div class="mx-auto flex w-full max-w-3xl flex-1 flex-col gap-3 p-4">
      <div>
        <h2 class="font-semibold">Preferences</h2>
        <p class="text-muted-foreground text-sm">How the app looks and behaves.</p>
      </div>
      <label class="grid max-w-xs gap-1.5 text-sm">
        <span class="font-medium">Default page budget</span>
        <select class="border-input h-8 rounded-md border bg-transparent px-2" [value]="budget()" (change)="setBudget($any($event.target).value)">
          <option value="1">1 page</option>
          <option value="2">2 pages</option>
        </select>
      </label>
      <label class="grid max-w-xs gap-1.5 text-sm">
        <span class="font-medium">Theme</span>
        <select class="border-input h-8 rounded-md border bg-transparent px-2" [value]="theme()" (change)="setTheme($any($event.target).value)">
          <option value="system">System</option>
          <option value="light">Light</option>
          <option value="dark">Dark</option>
        </select>
      </label>
      <div class="mt-4">
        <h2 class="font-semibold">Integrations</h2>
        <p class="text-muted-foreground text-sm">Connect outside accounts that feed the Jobs page.</p>
      </div>
      <app-gmail-card />
      @if (account()) {
        <div class="mt-4 flex items-center justify-between gap-3">
          <div>
            <h2 class="font-semibold">Account</h2>
            <p class="text-muted-foreground text-sm">Signed in as {{ account() }}.</p>
          </div>
          <button type="button" class="border-input h-8 rounded-md border px-3 text-sm" (click)="signOut()">Sign out</button>
        </div>
      }
      <p class="text-muted-foreground mt-auto pt-6 text-center text-xs">Version {{ version() }}</p>
    </div>
  `,
})
export class SettingsPageComponent implements OnInit {
  private readonly dark = inject(ZardDarkMode);
  protected readonly budget = signal('1');
  protected readonly theme = signal<DarkModeOptions>(this.dark.currentTheme());

  protected readonly version = signal('…');
  protected readonly account = signal<string | null>(null);

  ngOnInit(): void {
    void platform.version().then((v) => this.version.set(v), () => this.version.set('unknown'));
    void platform.auth.init().then((a) => this.account.set(a?.signedIn ? (a.email ?? 'your account') : null), () => undefined);
    void api.getUiSetting(BUDGET_SETTING).then((v) => this.budget.set(v === '2' ? '2' : '1'), () => undefined);
  }

  setBudget(v: string): void {
    this.budget.set(v);
    void api.setUiSetting(BUDGET_SETTING, v).catch(() => undefined);
  }

  async signOut(): Promise<void> {
    await platform.auth.signOut();
    location.assign('/');
  }

  setTheme(v: DarkModeOptions): void {
    this.theme.set(v);
    this.dark.toggleTheme(v);
  }
}
