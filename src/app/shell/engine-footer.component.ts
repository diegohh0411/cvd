import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { RouterLink } from '@angular/router';

import { ZardSidebarImports } from '@/shared/components/sidebar/sidebar.imports';
import { EngineStore } from '../core/stores/engine.store';

@Component({
  selector: 'app-engine-footer',
  imports: [...ZardSidebarImports, RouterLink],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <ul z-sidebar-menu>
      <li z-sidebar-menu-item>
        <a
          z-sidebar-menu-button
          routerLink="/models"
          [queryParams]="{ tab: 'engine' }"
          [zTooltip]="text()"
          aria-live="polite"
          data-testid="engine-footer"
        >
          <span class="flex size-4 shrink-0 items-center justify-center" aria-hidden="true">
            <span class="size-2 rounded-full" [class]="dot()"></span>
          </span>
          <span class="truncate text-xs">{{ text() }}</span>
        </a>
      </li>
    </ul>
  `,
})
export class EngineFooterComponent {
  private readonly engine = inject(EngineStore);
  protected readonly text = computed(() =>
    this.engine.status().state === 'none' ? 'No model active' : `${this.engine.modelLabel()} · ${this.engine.stateLabel()}`,
  );
  protected readonly dot = computed(() => {
    switch (this.engine.status().state) {
      case 'idle':
      case 'ready':
        return 'bg-emerald-500';
      case 'error':
      case 'missing':
        return 'bg-destructive';
      default:
        return 'bg-muted-foreground';
    }
  });
}
