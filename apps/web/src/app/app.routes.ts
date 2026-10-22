import { Routes } from '@angular/router';

import { ShellComponent } from './shell/shell.component';

/** Where an old `/models?tab=` link lands now. */
export function modelsRedirect(tab: string | undefined): string {
  return '/ai';
}

export const routes: Routes = [
  {
    path: '',
    component: ShellComponent,
    children: [
      { path: '', pathMatch: 'full', redirectTo: 'jobs' },
      { path: 'inbox', loadComponent: () => import('./features/inbox/inbox-page.component').then((m) => m.InboxPageComponent) },
      { path: 'jobs', loadComponent: () => import('./features/jobs/jobs-page.component').then((m) => m.JobsPageComponent) },
      { path: 'jobs/new', loadComponent: () => import('./features/jobs/new-job.component').then((m) => m.NewJobComponent) },
      {
        path: 'jobs/:jobId',
        loadComponent: () => import('./features/workspace/workspace-page.component').then((m) => m.WorkspacePageComponent),
      },
      { path: 'exports', loadComponent: () => import('./features/jobs/exports-page.component').then((m) => m.ExportsPageComponent) },
      {
        path: 'library/skills',
        loadComponent: () => import('./features/library/skills-page.component').then((m) => m.SkillsPageComponent),
      },
      {
        path: 'library/:kind',
        loadComponent: () => import('./features/library/library-page.component').then((m) => m.LibraryPageComponent),
      },
      { path: 'profile', loadComponent: () => import('./features/profile/profile-page.component').then((m) => m.ProfilePageComponent) },
      { path: 'ai', pathMatch: 'full', loadComponent: () => import('./features/ai/ai-models-page.component').then((m) => m.AiModelsPageComponent) },
      { path: 'ai/downloads', redirectTo: 'ai' },
      { path: 'ai/roles', redirectTo: 'ai' },
      { path: 'ai/providers', redirectTo: 'ai' },
      { path: 'ai/models', redirectTo: 'ai' },
      { path: 'models', redirectTo: ({ queryParams }) => modelsRedirect(queryParams['tab']) },
      {
        path: 'settings',
        loadComponent: () => import('./features/settings/settings-page.component').then((m) => m.SettingsPageComponent),
      },
      { path: '**', redirectTo: 'jobs' },
    ],
  },
];
