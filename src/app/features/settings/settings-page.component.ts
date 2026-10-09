import { ChangeDetectionStrategy, Component, computed, effect, inject, OnInit, signal, untracked } from '@angular/core';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { type DarkModeOptions, ZardDarkMode } from '@/shared/services/dark-mode';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { Profile } from '../../core/ipc/types';
import { ProfileStore } from '../../core/stores/profile.store';
import { PageHeaderComponent } from '../../shell/page-header.component';
import { BUDGET_SETTING } from '../workspace/workspace.store';
import { ModelRolesComponent } from './model-roles.component';
import { ProviderKeyComponent } from './provider-key.component';
import { cleanProfile, ProfileFormComponent, profileErrors } from './profile-form.component';

@Component({
  selector: 'app-settings-page',
  imports: [PageHeaderComponent, ProfileFormComponent, ProviderKeyComponent, ModelRolesComponent, ZardButtonComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'Settings' }]" />
    <div class="mx-auto flex w-full max-w-3xl flex-1 flex-col gap-8 p-4">
      <section class="grid gap-3">
        <div>
          <h2 class="font-semibold">Profile</h2>
          <p class="text-muted-foreground text-sm">Printed in the CV header.</p>
        </div>
        <app-profile-form [profile]="draft()" (profileChange)="draft.set($event)" />
        @if (touched() && errors().length) {
          <p class="text-destructive text-sm" role="alert">{{ errors().join(' ') }}</p>
        }
        <div><button z-button (click)="saveProfile()" [zLoading]="saving()">Save profile</button></div>
      </section>

      <section class="grid gap-3">
        <div>
          <h2 class="font-semibold">Providers</h2>
          <p class="text-muted-foreground text-sm">API keys for online models and web search.</p>
        </div>
        <app-provider-key provider="openrouter" />
        <app-provider-key provider="exa" />
      </section>

      <section class="grid gap-3">
        <div>
          <h2 class="font-semibold">Models</h2>
          <p class="text-muted-foreground text-sm">Which model does each job.</p>
        </div>
        <app-model-roles />
      </section>

      <section class="grid gap-3">
        <h2 class="font-semibold">Preferences</h2>
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
      </section>
    </div>
  `,
})
export class SettingsPageComponent implements OnInit {
  private readonly profile = inject(ProfileStore);
  private readonly dark = inject(ZardDarkMode);
  private readonly toast = inject(ZardSonnerService);
  protected readonly draft = signal<Profile>(this.profile.profile());
  protected readonly saving = signal(false);
  protected readonly touched = signal(false);
  protected readonly budget = signal('1');
  protected readonly theme = signal<DarkModeOptions>(this.dark.currentTheme());
  protected readonly errors = computed(() => profileErrors(this.draft()));

  constructor() {
    // Follow the stored profile until the user starts editing.
    effect(() => {
      const p = this.profile.profile();
      if (!untracked(this.touched)) this.draft.set(p);
    });
  }

  ngOnInit(): void {
    void api.getUiSetting(BUDGET_SETTING).then((v) => this.budget.set(v === '2' ? '2' : '1'), () => undefined);
  }

  async saveProfile(): Promise<void> {
    this.touched.set(true);
    if (this.errors().length) return;
    this.saving.set(true);
    try {
      await this.profile.save(cleanProfile(this.draft()));
      this.toast.success('Profile saved');
    } catch (e) {
      this.toast.error('Could not save', { description: messageFor(e) });
    } finally {
      this.saving.set(false);
    }
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
