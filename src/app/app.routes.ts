import { Routes } from '@angular/router';

import { ShellComponent } from './shell/shell.component';

export const routes: Routes = [
  {
    path: '',
    component: ShellComponent,
    children: [
      { path: '', pathMatch: 'full', redirectTo: 'jobs' },
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
      { path: 'models', loadComponent: () => import('./features/models/models-page.component').then((m) => m.ModelsPageComponent) },
      {
        path: 'settings',
        loadComponent: () => import('./features/settings/settings-page.component').then((m) => m.SettingsPageComponent),
      },
      { path: '**', redirectTo: 'jobs' },
    ],
  },
];
