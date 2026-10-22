import { DestroyRef } from '@angular/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

import type {
  MatchProgressEvent,
  MatchStage,
  ResearchEvent,
  ScanFinished,
  ScanProgress,
  WatcherStatus,
} from './types';

export const MATCH_EVENT = 'match://progress';
export const ENGINE_EVENT = 'engine://status';
export const RESEARCH_EVENT = 'company://research';
export const SCAN_PROGRESS_EVENT = 'scan://progress';
export const SCAN_FINISHED_EVENT = 'scan://finished';
export const WATCHER_EVENT = 'mail://watcher';
/** An inbox row was written, re-checked, read, applied, dismissed or undone. */
export const INBOX_CHANGED_EVENT = 'inbox://changed';
/** A job's tracking changed outside the main window (auto-apply marked it Applied). */
export const JOB_UPDATED_EVENT = 'job://updated';
/** Tables written by anyone (the app, auto-apply, the mail watcher, the MCP server). */
export const DB_CHANGED_EVENT = 'db://changed';

/** Database tables, as named by the change feed. */
export type Table =
  | 'profile' | 'entry' | 'bullet' | 'skill' | 'entry_skill'
  | 'job_description' | 'match_run' | 'match_score' | 'cv'
  | 'settings' | 'secrets' | 'job_status' | 'company' | 'company_alias'
  | 'scan_run' | 'processed_email' | 'apply_memory' | 'job_view';

/** Calls `cb` when any of `tables` changes; bursts within `debounceMs` coalesce into one call. */
export function onTablesChanged(tables: Table[], cb: () => void, debounceMs = 150): Promise<UnlistenFn> {
  const watched = new Set<string>(tables);
  let timer: ReturnType<typeof setTimeout> | null = null;
  return listen<{ tables: string[] }>(DB_CHANGED_EVENT, ({ payload }) => {
    if (!payload.tables.some((t) => watched.has(t))) return;
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      cb();
    }, debounceMs);
  });
}

/** Progress for one run. Subscribe before starting the run so nothing is missed. */
export function onMatchProgress(runId: number | (() => number | null), cb: (e: MatchStage) => void): Promise<UnlistenFn> {
  const current = typeof runId === 'function' ? runId : () => runId;
  return listen<MatchProgressEvent>(MATCH_EVENT, ({ payload }) => {
    if (payload.run_id === current()) cb(payload.event);
  });
}

export function onAnyMatchProgress(cb: (e: MatchProgressEvent) => void): Promise<UnlistenFn> {
  return listen<MatchProgressEvent>(MATCH_EVENT, ({ payload }) => cb(payload));
}

export function onResearch(cb: (e: ResearchEvent) => void): Promise<UnlistenFn> {
  return listen<ResearchEvent>(RESEARCH_EVENT, ({ payload }) => cb(payload));
}

export function onScanProgress(cb: (e: ScanProgress) => void): Promise<UnlistenFn> {
  return listen<ScanProgress>(SCAN_PROGRESS_EVENT, ({ payload }) => cb(payload));
}

export function onScanFinished(cb: (e: ScanFinished) => void): Promise<UnlistenFn> {
  return listen<ScanFinished>(SCAN_FINISHED_EVENT, ({ payload }) => cb(payload));
}

export function onWatcherStatus(cb: (e: WatcherStatus) => void): Promise<UnlistenFn> {
  return listen<WatcherStatus>(WATCHER_EVENT, ({ payload }) => cb(payload));
}

export function onInboxChanged(cb: () => void): Promise<UnlistenFn> {
  return listen(INBOX_CHANGED_EVENT, () => cb());
}

export function onJobUpdated(cb: (e: { job_id: number; status_id: number | null }) => void): Promise<UnlistenFn> {
  return listen<{ job_id: number; status_id: number | null }>(JOB_UPDATED_EVENT, ({ payload }) => cb(payload));
}

export function onEngineStatus(cb: () => void): Promise<UnlistenFn> {
  return listen(ENGINE_EVENT, () => cb());
}

/** Ties a `listen()` subscription to an injection context's lifetime. */
export function untilDestroyed(destroyRef: DestroyRef, sub: Promise<UnlistenFn>): Promise<UnlistenFn> {
  let destroyed = false;
  destroyRef.onDestroy(() => {
    destroyed = true;
    void sub.then((un) => un());
  });
  return sub.then((un) => {
    if (destroyed) un();
    return un;
  });
}
