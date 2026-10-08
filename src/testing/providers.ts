import { EnvironmentProviders, Provider } from '@angular/core';
import { provideRouter } from '@angular/router';

import { ZardSidebarService } from '@/shared/components/sidebar/sidebar.service';

/** What a routed page needs outside the shell (the sidebar trigger's service). */
export function pageProviders(): (Provider | EnvironmentProviders)[] {
  return [provideRouter([]), ZardSidebarService];
}
