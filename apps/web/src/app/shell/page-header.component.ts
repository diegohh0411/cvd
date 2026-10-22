import { ChangeDetectionStrategy, Component, input } from '@angular/core';

import { ZardBreadcrumbImports } from '@/shared/components/breadcrumb/breadcrumb.imports';
import { ZardSeparatorComponent } from '@/shared/components/separator/separator.component';
import { ZardSidebarImports } from '@/shared/components/sidebar/sidebar.imports';

export interface Crumb {
  label: string;
  link?: string | (string | number)[];
}

/** Inset header: sidebar trigger, breadcrumb, and projected actions. */
@Component({
  selector: 'app-page-header',
  imports: [...ZardSidebarImports, ...ZardBreadcrumbImports, ZardSeparatorComponent],
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { class: 'contents' },
  template: `
    <header
      class="bg-background sticky top-0 z-10 flex h-14 shrink-0 items-center gap-2 border-b px-4 transition-[width,height] ease-linear group-has-data-[collapsible=icon]/sidebar-wrapper:h-12"
    >
      <button z-sidebar-trigger class="-ml-1" aria-label="Toggle sidebar"></button>
      <z-separator zOrientation="vertical" class="mr-2 data-[orientation=vertical]:h-4 data-[orientation=vertical]:self-center" />
      <z-breadcrumb class="min-w-0 flex-1">
        @for (c of crumbs(); track $index; let last = $last) {
          <z-breadcrumb-item [class.hidden]="!last" class="md:block">
            @if (last || !c.link) {
              <span z-breadcrumb-page class="truncate">{{ c.label }}</span>
            } @else {
              <a z-breadcrumb-link [routerLink]="c.link">{{ c.label }}</a>
            }
          </z-breadcrumb-item>
          @if (!last) {
            <z-breadcrumb-separator class="hidden md:block" />
          }
        }
      </z-breadcrumb>
      <div class="flex shrink-0 items-center gap-2">
        <ng-content />
      </div>
    </header>
  `,
})
export class PageHeaderComponent {
  readonly crumbs = input.required<Crumb[]>();
}
