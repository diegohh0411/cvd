import { DestroyRef } from '@angular/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';

import type {
  DownloadEvent,
  MatchProgressEvent,
  MatchStage,
  ResearchEvent,
  ScanFinished,
  ScanProgress,
} from './types';

export const MATCH_EVENT = 'match://progress';
export const DOWNLOAD_EVENT = 'model://download';
export const ENGINE_EVENT = 'engine://status';
export const RESEARCH_EVENT = 'company://research';
export const SCAN_PROGRESS_EVENT = 'scan://progress';
export const SCAN_FINISHED_EVENT = 'scan://finished';

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

export function onDownload(cb: (e: DownloadEvent) => void): Promise<UnlistenFn> {
  return listen<DownloadEvent>(DOWNLOAD_EVENT, ({ payload }) => cb(payload));
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
