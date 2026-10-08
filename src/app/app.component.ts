import { ChangeDetectionStrategy, Component, effect, inject, OnInit } from '@angular/core';
import { RouterOutlet } from '@angular/router';

import { ZardSonnerComponent } from '@/shared/components/sonner/sonner.component';
import { ZardDarkMode } from '@/shared/services/dark-mode';
import { EngineStore } from './core/stores/engine.store';
import { JobsStore } from './core/stores/jobs.store';
import { LibraryStore } from './core/stores/library.store';
import { ModelsStore } from './core/stores/models.store';
import { ProfileStore } from './core/stores/profile.store';
import { FirstRunDialogComponent } from './features/onboarding/first-run-dialog.component';
import { FirstRunService } from './features/onboarding/first-run.service';

@Component({
  selector: 'app-root',
  imports: [RouterOutlet, ZardSonnerComponent, FirstRunDialogComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <router-outlet />
    <z-sonner />
    <app-first-run-dialog />
  `,
})
export class AppComponent implements OnInit {
  private readonly profile = inject(ProfileStore);
  private readonly library = inject(LibraryStore);
  private readonly jobs = inject(JobsStore);
  private readonly models = inject(ModelsStore);
  private readonly engine = inject(EngineStore);
  private readonly firstRun = inject(FirstRunService);

  constructor() {
    inject(ZardDarkMode).init();
    effect(() => {
      if (this.profile.isEmpty()) this.firstRun.open();
    });
  }

  ngOnInit(): void {
    void this.profile.load();
    void this.library.load();
    void this.jobs.load();
    void this.models.load();
    void this.engine.load();
  }
}
