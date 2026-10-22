import { ChangeDetectionStrategy, Component } from '@angular/core';
import { RouterOutlet } from '@angular/router';

import { ZardSidebarImports } from '@/shared/components/sidebar/sidebar.imports';
import { AppSidebarComponent } from './app-sidebar.component';

@Component({
  selector: 'app-shell',
  imports: [...ZardSidebarImports, AppSidebarComponent, RouterOutlet],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <z-sidebar-provider zDefaultOpen="true">
      <app-sidebar />
      <main z-sidebar-inset class="min-w-0">
        <router-outlet />
      </main>
    </z-sidebar-provider>
  `,
})
export class ShellComponent {}
