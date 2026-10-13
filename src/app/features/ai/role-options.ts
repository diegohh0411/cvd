import type { ModelInfo, ProviderModels } from '../../core/ipc/types';
import { formatBytes } from '../../core/stores/models.store';

/** The job a model does. */
export type Role = 'system-one' | 'research';
/** Where a model runs. */
export type ProviderKind = 'local' | 'openrouter' | 'subq';
export type RoleStatus = 'ready' | 'needs-setup' | 'error';

export interface RoleOption {
  id: string;
  label: string;
  installed: boolean;
  detail: string;
}

export const REMOTE_PREFIX = 'openrouter:';
export const SUBQ_PREFIX = 'subq:';
/** Set to '1' once the user has acknowledged that online models receive their data. */
export const REMOTE_NOTICE_SETTING = 'ui.remote_notice_ack';

export function formatPrice(perMtok: number | null): string {
  if (perMtok === null) return '';
  if (perMtok === 0) return 'free';
  return `$${Number(perMtok.toPrecision(3))} / M tokens`;
}

export function providerOf(id: string | null): ProviderKind {
  if (id?.startsWith(REMOTE_PREFIX)) return 'openrouter';
  if (id?.startsWith(SUBQ_PREFIX)) return 'subq';
  return 'local';
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
  hasOpenRouterKey: boolean;
  hasSubQKey?: boolean;
  /** Research only; undefined means the role needs no Exa key. */
  hasExaKey?: boolean;
  engineError?: boolean;
}): RoleStatus {
  if (i.engineError) return 'error';
  if (!i.active || i.hasExaKey === false) return 'needs-setup';
  if (providerOf(i.active) === 'openrouter') return i.hasOpenRouterKey ? 'ready' : 'needs-setup';
  if (providerOf(i.active) === 'subq') return i.hasSubQKey ? 'ready' : 'needs-setup';
  return i.installed.some((m) => m.spec.id === i.active) ? 'ready' : 'needs-setup';
}
