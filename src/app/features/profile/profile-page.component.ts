import { ChangeDetectionStrategy, Component, computed, effect, inject, OnInit, signal, untracked, viewChild } from '@angular/core';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { messageFor } from '../../core/ipc/errors';
import type { Profile } from '../../core/ipc/types';
import { ProfileStore } from '../../core/stores/profile.store';
import { PageHeaderComponent } from '../../shell/page-header.component';
import { ApplyDefaultsComponent } from './apply-defaults.component';
import { cleanProfile, ProfileFormComponent, profileErrors } from './profile-form.component';

@Component({
  selector: 'app-profile-page',
  imports: [ApplyDefaultsComponent, PageHeaderComponent, ProfileFormComponent, ZardButtonComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <app-page-header [crumbs]="[{ label: 'Profile' }]" />
    <div class="mx-auto flex w-full max-w-3xl flex-1 flex-col gap-3 p-4">
      <div>
        <h2 class="font-semibold">Profile</h2>
        <p class="text-muted-foreground text-sm">Printed in the CV header.</p>
      </div>
      <app-profile-form [profile]="draft()" (profileChange)="draft.set($event)" />
      @if (touched() && errors().length) {
        <p class="text-destructive text-sm" role="alert">{{ errors().join(' ') }}</p>
      }
      <div><button z-button (click)="save()" [zLoading]="saving()">Save profile</button></div>
      <app-apply-defaults class="mt-6 border-t pt-6" />
    </div>
  `,
})
export class ProfilePageComponent implements OnInit {
  private readonly profile = inject(ProfileStore);
  private readonly toast = inject(ZardSonnerService);
  protected readonly draft = signal<Profile>(this.profile.profile());
  protected readonly saving = signal(false);
  protected readonly touched = signal(false);
  protected readonly errors = computed(() => profileErrors(this.draft()));
  private readonly applyDefaults = viewChild(ApplyDefaultsComponent);

  constructor() {
    // Follow the stored profile until the user starts editing.
    effect(() => {
      const p = this.profile.profile();
      if (!untracked(this.touched)) this.draft.set(p);
    });
  }

  ngOnInit(): void {
    if (!this.profile.loaded()) void this.profile.load();
  }

  async save(): Promise<void> {
    this.touched.set(true);
    if (this.errors().length) return;
    this.saving.set(true);
    try {
      await this.profile.save(cleanProfile(this.draft()));
      this.toast.success('Profile saved');
      void this.applyDefaults()?.refreshFacts().catch(() => undefined);
    } catch (e) {
      this.toast.error('Could not save', { description: messageFor(e) });
    } finally {
      this.saving.set(false);
    }
  }
}
