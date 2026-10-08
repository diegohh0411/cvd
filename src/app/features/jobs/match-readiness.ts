import { computed, inject, Injectable } from '@angular/core';

import { EngineStore } from '../../core/stores/engine.store';
import { LibraryStore } from '../../core/stores/library.store';

export interface Blocker {
  message: string;
  link: string;
  queryParams?: Record<string, string>;
  action: string;
}

/** Why Match is disabled right now, if it is (spec §8). */
@Injectable({ providedIn: 'root' })
export class MatchReadiness {
  private readonly library = inject(LibraryStore);
  private readonly engine = inject(EngineStore);

  readonly blocker = computed<Blocker | null>(() => {
    if (this.library.isEmpty()) {
      return { message: 'Your library is empty. Add at least one entry to match against.', link: '/library/experience', action: 'Open Library' };
    }
    const state = this.engine.status().state;
    if (state === 'none' || state === 'missing') {
      return {
        message: state === 'none' ? 'No decision model is active yet.' : 'The active model is not installed.',
        link: '/models',
        queryParams: { tab: 'download' },
        action: 'Download a model',
      };
    }
    return null;
  });
}
