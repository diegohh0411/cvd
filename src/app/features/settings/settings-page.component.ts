import { ChangeDetectionStrategy, Component, inject, OnInit, signal } from '@angular/core';

import { type DarkModeOptions, ZardDarkMode } from '@/shared/services/dark-mode';
import * as api from '../../core/ipc/commands';
import { PageHeaderComponent } from '../../shell/page-header.component';
import { BUDGET_SETTING } from '../workspace/workspace.store';

@Component({
  selector: 'app-settings-page',
  imports: [PageHeaderComponent],
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
    </div>
  `,
})
export class SettingsPageComponent implements OnInit {
  private readonly dark = inject(ZardDarkMode);
  protected readonly budget = signal('1');
  protected readonly theme = signal<DarkModeOptions>(this.dark.currentTheme());

  ngOnInit(): void {
    void api.getUiSetting(BUDGET_SETTING).then((v) => this.budget.set(v === '2' ? '2' : '1'), () => undefined);
  }

  setBudget(v: string): void {
    this.budget.set(v);
    void api.setUiSetting(BUDGET_SETTING, v).catch(() => undefined);
  }

  setTheme(v: DarkModeOptions): void {
    this.theme.set(v);
    this.dark.toggleTheme(v);
  }
}
