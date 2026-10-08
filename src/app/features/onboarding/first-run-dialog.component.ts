import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { Router } from '@angular/router';

import { ZardButtonComponent } from '@/shared/components/button/button.component';
import { ZardDialogImports } from '@/shared/components/dialog/dialog.imports';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { messageFor } from '../../core/ipc/errors';
import type { EntryInput, Profile } from '../../core/ipc/types';
import { EngineStore } from '../../core/stores/engine.store';
import { emptyInput, LibraryStore } from '../../core/stores/library.store';
import { ModelsStore } from '../../core/stores/models.store';
import { ProfileStore } from '../../core/stores/profile.store';
import { cleanInput, EntryFormComponent, entryErrors } from '../library/entry-form.component';
import { ModelCardsComponent } from '../models/model-cards.component';
import { cleanProfile, ProfileFormComponent, profileErrors } from '../settings/profile-form.component';
import { FirstRunService } from './first-run.service';

export const FIRST_RUN_STEPS = ['Profile', 'Model', 'First entry'] as const;

/** First run (spec §7): profile → model (download continues in the background) → first entry. */
@Component({
  selector: 'app-first-run-dialog',
  imports: [...ZardDialogImports, ZardButtonComponent, ProfileFormComponent, ModelCardsComponent, EntryFormComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <z-dialog [zVisible]="firstRun.visible()" (zVisibleChange)="!$event && firstRun.close()" class="sm:max-w-2xl" [zMaskClosable]="false">
      <z-dialog-header>
        <z-dialog-title>Welcome to CV Decisions</z-dialog-title>
        <z-dialog-description>
          Step {{ step() + 1 }} of {{ steps.length }} · {{ steps[step()] }}
        </z-dialog-description>
      </z-dialog-header>
      <ol class="flex gap-2" aria-hidden="true">
        @for (s of steps; track s; let i = $index) {
          <li class="h-1 flex-1 rounded-full" [class]="i <= step() ? 'bg-primary' : 'bg-muted'"></li>
        }
      </ol>
      <div class="-mx-1 max-h-[60vh] overflow-y-auto px-1 py-2" data-testid="first-run-body">
        @switch (step()) {
          @case (0) {
            <p class="text-muted-foreground mb-3 text-sm">This goes in your CV header. Everything stays on this computer.</p>
            <app-profile-form [profile]="profile()" (profileChange)="profile.set($event)" />
          }
          @case (1) {
            <p class="text-muted-foreground mb-3 text-sm">
              A local model ranks your experience against each job.
              @if (models.ramGb(); as ram) {
                This computer has {{ ram }} GB of RAM{{ models.recommended() ? '; we recommend ' + models.recommended()!.label.replace(' (default)', '') + '.' : '.' }}
              }
              The download continues in the background.
            </p>
            <app-model-cards />
          }
          @case (2) {
            <p class="text-muted-foreground mb-3 text-sm">Add one job or project now; you can add the rest from the Library.</p>
            <app-entry-form [draft]="entry()" (draftChange)="entry.set($event)" />
          }
        }
        @if (touched() && errors().length) {
          <p class="text-destructive mt-3 text-sm" role="alert">{{ errors().join(' ') }}</p>
        }
      </div>
      <z-dialog-footer class="flex-row justify-between">
        <div>
          @if (step() > 0) {
            <button z-button zType="ghost" (click)="back()">Back</button>
          }
        </div>
        <div class="flex gap-2">
          @if (step() > 0) {
            <button z-button zType="outline" (click)="skip()">{{ step() === 1 ? 'Continue while it downloads' : 'Skip' }}</button>
          }
          <button z-button (click)="next()" [zLoading]="busy()">{{ step() === steps.length - 1 ? 'Finish' : 'Next' }}</button>
        </div>
      </z-dialog-footer>
    </z-dialog>
  `,
})
export class FirstRunDialogComponent {
  protected readonly firstRun = inject(FirstRunService);
  protected readonly models = inject(ModelsStore);
  private readonly engine = inject(EngineStore);
  private readonly profileStore = inject(ProfileStore);
  private readonly library = inject(LibraryStore);
  private readonly router = inject(Router);
  private readonly toast = inject(ZardSonnerService);

  protected readonly steps = FIRST_RUN_STEPS;
  readonly step = signal(0);
  protected readonly profile = signal<Profile>(this.profileStore.profile());
  protected readonly entry = signal<EntryInput>(emptyInput('experience'));
  protected readonly busy = signal(false);
  protected readonly touched = signal(false);
  protected readonly errors = computed(() => {
    switch (this.step()) {
      case 0:
        return profileErrors(this.profile());
      case 1:
        return this.engine.status().model_id ? [] : ['Download a model, or continue while it downloads.'];
      default:
        return entryErrors(this.entry());
    }
  });

  back(): void {
    this.touched.set(false);
    this.step.update((s) => Math.max(0, s - 1));
  }

  skip(): void {
    this.touched.set(false);
    if (this.step() === this.steps.length - 1) this.finish();
    else this.step.update((s) => s + 1);
  }

  async next(): Promise<void> {
    this.touched.set(true);
    if (this.errors().length) return;
    this.busy.set(true);
    try {
      if (this.step() === 0) await this.profileStore.save(cleanProfile(this.profile()));
      if (this.step() === 2) await this.library.save(cleanInput(this.entry()));
      this.touched.set(false);
      if (this.step() === this.steps.length - 1) this.finish();
      else this.step.update((s) => s + 1);
    } catch (e) {
      this.toast.error('Could not save', { description: messageFor(e) });
    } finally {
      this.busy.set(false);
    }
  }

  private finish(): void {
    this.firstRun.close();
    void this.router.navigate([this.library.isEmpty() ? '/library/experience' : '/jobs/new']);
  }
}
