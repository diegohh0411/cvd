import type { ProviderId } from '../../core/ipc/types';

/** The job a model does. */
export type Role = 'system-one' | 'research';

export interface ProviderDef {
  id: ProviderId;
  name: string;
  placeholder: string;
  hint: string;
  /** Slots whose picker lists this provider's models; empty for tools like Exa. */
  roles: Role[];
}

/** Every online service the app can hold a key for, in display order. */
export const PROVIDERS: readonly ProviderDef[] = [
  {
    id: 'openrouter',
    name: 'OpenRouter',
    placeholder: 'sk-or-...',
    hint: 'Lets you run online models. Matching or researching with them sends your CV and job text to OpenRouter.',
    roles: ['system-one', 'research'],
  },
  {
    id: 'subq',
    name: 'SubQ',
    placeholder: 'SubQ API key',
    hint: 'Lets you run SubQ Snap for decisions. Matching with it sends your CV and job text to SubQ.',
    roles: ['system-one'],
  },
  {
    id: 'typesafe',
    name: 'TypeSafe',
    placeholder: 'TypeSafe API key',
    hint: 'Lets you run Jev for decisions, billed to your TypeSafe credits. Matching with it sends your CV and job text to TypeSafe.',
    roles: ['system-one'],
  },
  {
    id: 'exa',
    name: 'Exa',
    placeholder: 'Exa API key',
    hint: 'Web search for company research. Search queries (company names) are sent to Exa.',
    roles: [],
  },
];

export const LOCAL_PROVIDER = { id: 'local', name: 'Local (llama.cpp)' } as const;

/** Providers that serve models for `role`. */
export const modelProviders = (role: Role): ProviderDef[] => PROVIDERS.filter((p) => p.roles.includes(role));

/** `openrouter~2` -> `openrouter`. */
export const kindOf = (id: string): string => id.split('~')[0];

export const providerName = (id: string): string => PROVIDERS.find((p) => p.id === kindOf(id))?.name ?? id;
