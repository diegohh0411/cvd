import { ChangeDetectionStrategy, Component, effect, inject, OnInit, signal } from '@angular/core';
import { RouterLink, RouterOutlet } from '@angular/router';

import { ZardSonnerComponent } from '@/shared/components/sonner/sonner.component';
import { ZardDarkMode } from '@/shared/services/dark-mode';
import { EngineStore } from './core/stores/engine.store';
import { JobsStore } from './core/stores/jobs.store';
import { LibraryStore } from './core/stores/library.store';
import { ProfileStore } from './core/stores/profile.store';
import { UpdateService } from './core/update.service';
import { FirstRunDialogComponent } from './features/onboarding/first-run-dialog.component';
import { FirstRunService } from './features/onboarding/first-run.service';
import * as api from './core/ipc/commands';

@Component({
  selector: 'app-root',
  imports: [RouterLink, RouterOutlet, ZardSonnerComponent, FirstRunDialogComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (localMigration()) {
      <div class="border-b bg-muted p-4 text-sm" role="status">
        Local models now run in a server you start yourself. Your old local model selections were cleared.
        <a routerLink="/ai" class="underline">Set up Local server in AI Models</a>.
        You can delete old GGUF downloads from the app data models/ folder to reclaim disk space.
        <button type="button" class="ml-3 underline" (click)="dismissMigration()">Dismiss</button>
      </div>
    }
    <router-outlet />
    <z-sonner />
    <app-first-run-dialog />
  `,
})
export class AppComponent implements OnInit {
  protected readonly localMigration = signal(false);
  protected async dismissMigration(): Promise<void> {
    await api.setUiSetting('ui.local_server_migration', '0');
    this.localMigration.set(false);
  }
  private readonly profile = inject(ProfileStore);
  private readonly library = inject(LibraryStore);
  private readonly jobs = inject(JobsStore);
  private readonly engine = inject(EngineStore);
  private readonly firstRun = inject(FirstRunService);
  private readonly updates = inject(UpdateService);

  constructor() {
    inject(ZardDarkMode).init();
    effect(() => {
      if (this.profile.isEmpty()) this.firstRun.open();
    });
  }

  ngOnInit(): void {
    void api.getUiSetting('ui.local_server_migration').then(v => this.localMigration.set(v === '1')).catch(() => undefined);
    void this.profile.load();
    void this.library.load();
    void this.jobs.load();
    void this.engine.load();
    void this.updates.checkOnStartup();
  }
}
