import type { ModelInfo, ProviderModels } from '../../core/ipc/types';
import { formatBytes } from '../../core/stores/models.store';

import type { Role } from './provider-registry';

export type { Role };
/** Where a model runs: `local`, or a provider id from the registry. */
export type ProviderKind = string;
export type RoleStatus = 'ready' | 'needs-setup' | 'error';

export interface RoleOption {
  id: string;
  label: string;
  installed: boolean;
  detail: string;
}

/** Set to '1' once the user has acknowledged that online models receive their data. */
export const REMOTE_NOTICE_SETTING = 'ui.remote_notice_ack';

export function formatPrice(perMtok: number | null): string {
  if (perMtok === null) return '';
  if (perMtok === 0) return 'free';
  return `$${Number(perMtok.toPrecision(3))} / M tokens`;
}

/** Remote ids are `<provider>:<model>`; local ids never contain a colon. */
export function providerOf(id: string | null): ProviderKind {
  const i = id?.indexOf(':') ?? -1;
  return id && i > 0 ? id.slice(0, i) : 'local';
}

const KIND: Record<Role, 'decision' | 'chat'> = { 'system-one': 'decision', research: 'chat' };

/** Every catalog model that can fill `role`, installed ones first. */
export function localOptions(role: Role, models: ModelInfo[]): RoleOption[] {
  return models
    .filter((m) => m.spec.kind === KIND[role])
    .sort((a, b) => Number(b.installed) - Number(a.installed))
    .map((m) => ({ id: m.spec.id, label: m.spec.label, installed: m.installed, detail: formatBytes(m.spec.size_bytes) }));
}

export function onlineOptions(lists: ProviderModels[]): RoleOption[] {
  return lists.flatMap((p) =>
    p.models.map((m) => ({ id: `${p.provider}:${m.id}`, label: m.display_name, installed: true, detail: formatPrice(m.price_per_mtok_in) })),
  );
}

export function roleStatus(i: {
  active: string | null;
  installed: ModelInfo[];
  /** Which providers have a saved key, by provider id. */
  keys: Record<string, boolean>;
  /** Research only; undefined means the role needs no Exa key. */
  hasExaKey?: boolean;
  engineError?: boolean;
}): RoleStatus {
  if (i.engineError) return 'error';
  if (!i.active || i.hasExaKey === false) return 'needs-setup';
  const provider = providerOf(i.active);
  if (provider !== 'local') return i.keys[provider] ? 'ready' : 'needs-setup';
  return i.installed.some((m) => m.spec.id === i.active) ? 'ready' : 'needs-setup';
}
