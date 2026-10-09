import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { RouterLink, RouterLinkActive } from '@angular/router';
import { NgIcon, provideIcons } from '@ng-icons/core';
import {
  lucideBot,
  lucideBriefcase,
  lucideChevronRight,
  lucideFileText,
  lucideLibrary,
  lucideMoreHorizontal,
  lucideSettings2,
  lucideUserRound,
} from '@ng-icons/lucide';

import { ZardCollapsibleImports } from '@/shared/components/collapsible/collapsible.imports';
import { ZardSidebarImports } from '@/shared/components/sidebar/sidebar.imports';
import { jobLabel, JobsStore } from '../core/stores/jobs.store';
import { ProfileStore } from '../core/stores/profile.store';
import { EngineFooterComponent } from './engine-footer.component';

interface NavItem {
  readonly title: string;
  readonly icon: string;
  readonly base: string;
  readonly items: readonly { title: string; link: string; query?: Record<string, string> }[];
}

export const NAV: readonly NavItem[] = [
  {
    title: 'Jobs',
    icon: 'lucideBriefcase',
    base: '/jobs',
    items: [
      { title: 'All jobs', link: '/jobs' },
      { title: '+ New job', link: '/jobs/new' },
      { title: 'Exports', link: '/exports' },
    ],
  },
  {
    title: 'Library',
    icon: 'lucideLibrary',
    base: '/library',
    items: [
      { title: 'Experience', link: '/library/experience' },
      { title: 'Projects', link: '/library/project' },
      { title: 'Education', link: '/library/education' },
      { title: 'Volunteer', link: '/library/volunteer' },
      { title: 'Skills', link: '/library/skills' },
    ],
  },
  {
    title: 'AI',
    icon: 'lucideBot',
    base: '/ai',
    items: [
      { title: 'Roles', link: '/ai/roles' },
      { title: 'Models', link: '/ai/models' },
      { title: 'Providers', link: '/ai/providers' },
    ],
  },
];

@Component({
  selector: 'app-sidebar',
  imports: [...ZardSidebarImports, ...ZardCollapsibleImports, NgIcon, RouterLink, RouterLinkActive, EngineFooterComponent],
  viewProviders: [
    provideIcons({
      lucideBot,
      lucideBriefcase,
      lucideChevronRight,
      lucideFileText,
      lucideLibrary,
      lucideMoreHorizontal,
      lucideSettings2,
      lucideUserRound,
    }),
  ],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <z-sidebar zCollapsible="icon">
      <div z-sidebar-header>
        <ul z-sidebar-menu>
          <li z-sidebar-menu-item>
            <a z-sidebar-menu-button zSize="lg" routerLink="/jobs" [zTooltip]="'CV Decisions'">
              <div
                class="bg-primary text-primary-foreground flex aspect-square size-8 items-center justify-center rounded-lg"
              >
                <ng-icon name="lucideFileText" class="size-4" />
              </div>
              <div class="grid flex-1 text-left text-sm leading-tight">
                <span class="truncate font-semibold">CV Decisions</span>
                <span class="text-muted-foreground truncate text-xs">{{ profileName() }}</span>
              </div>
            </a>
          </li>
        </ul>
      </div>

      <z-sidebar-content>
        <div z-sidebar-group>
          <div z-sidebar-group-label>Workspace</div>
          <ul z-sidebar-menu>
            @for (item of nav; track item.title) {
              <li z-sidebar-menu-item z-collapsible class="group/collapsible" [zOpen]="true">
                <button z-collapsible-trigger z-sidebar-menu-button [zTooltip]="item.title">
                  <ng-icon [name]="item.icon" />
                  <span>{{ item.title }}</span>
                  <ng-icon
                    name="lucideChevronRight"
                    class="ml-auto transition-transform duration-200 group-data-[state=open]/collapsible:rotate-90"
                  />
                </button>
                <z-collapsible-content>
                  <ul z-sidebar-menu-sub>
                    @for (sub of item.items; track sub.title) {
                      <li z-sidebar-menu-sub-item>
                        <a
                          z-sidebar-menu-sub-button
                          [routerLink]="sub.link"
                          [queryParams]="sub.query"
                          routerLinkActive
                          #rla="routerLinkActive"
                          [routerLinkActiveOptions]="{
                            paths: 'exact',
                            queryParams: sub.query ? 'subset' : 'ignored',
                            matrixParams: 'ignored',
                            fragment: 'ignored',
                          }"
                          [zActive]="rla.isActive"
                        >
                          <span>{{ sub.title }}</span>
                        </a>
                      </li>
                    }
                  </ul>
                </z-collapsible-content>
              </li>
            }
            <li z-sidebar-menu-item>
              <a
                z-sidebar-menu-button
                routerLink="/profile"
                routerLinkActive
                #profile="routerLinkActive"
                [zActive]="profile.isActive"
                [zTooltip]="'Profile'"
              >
                <ng-icon name="lucideUserRound" />
                <span>Profile</span>
              </a>
            </li>
            <li z-sidebar-menu-item>
              <a
                z-sidebar-menu-button
                routerLink="/settings"
                routerLinkActive
                #settings="routerLinkActive"
                [zActive]="settings.isActive"
                [zTooltip]="'Settings'"
              >
                <ng-icon name="lucideSettings2" />
                <span>Settings</span>
              </a>
            </li>
          </ul>
        </div>

        <div z-sidebar-group class="group-data-[collapsible=icon]:hidden">
          <div z-sidebar-group-label>Recent jobs</div>
          <ul z-sidebar-menu>
            @for (job of jobs.recent(); track job.id) {
              <li z-sidebar-menu-item>
                <a
                  z-sidebar-menu-button
                  [routerLink]="['/jobs', job.id]"
                  routerLinkActive
                  #r="routerLinkActive"
                  [zActive]="r.isActive"
                >
                  <span class="truncate">{{ label(job) }}</span>
                </a>
              </li>
            } @empty {
              <li class="text-muted-foreground px-2 py-1 text-xs">No jobs yet</li>
            }
            @if (jobs.jobs().length > jobs.recent().length) {
              <li z-sidebar-menu-item>
                <a z-sidebar-menu-button routerLink="/jobs" class="text-sidebar-foreground/70">
                  <ng-icon name="lucideMoreHorizontal" class="text-sidebar-foreground/70" />
                  <span>More</span>
                </a>
              </li>
            }
          </ul>
        </div>
      </z-sidebar-content>

      <div z-sidebar-footer>
        <app-engine-footer />
      </div>

      <button z-sidebar-rail aria-label="Toggle sidebar"></button>
    </z-sidebar>
  `,
})
export class AppSidebarComponent {
  protected readonly jobs = inject(JobsStore);
  private readonly profile = inject(ProfileStore);
  protected readonly nav = NAV;
  protected readonly label = jobLabel;
  protected readonly profileName = computed(() => this.profile.profile().name || 'Your profile');
}
