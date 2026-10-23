// Types the webview sees. Mirrors of Rust types are generated into ./bindings.ts
// (crates/cvd-core/src/bindings.rs); this file adds the UI-defined shapes and
// event payloads the backend builds as ad-hoc JSON. Field names stay snake_case
// because serde serializes them as-is.
import type { AppError, Company, EntryKind, ResearchStep, ScanSource, StatusColor } from './bindings';

export type * from './bindings';

export const ENTRY_KINDS: readonly EntryKind[] = ['experience', 'project', 'education', 'volunteer'];

// ---- Jobs page views: `filters`/`sorts` are opaque JSON to the backend ----

export type FieldKey = 'company' | 'title' | 'status' | 'url' | 'created';
export type FilterOp = 'contains' | 'not_contains' | 'is' | 'is_not' | 'empty' | 'not_empty' | 'before' | 'after';

export interface JobFilter {
  field: FieldKey;
  op: FilterOp;
  /** Text for text fields, status ids for status, YYYY-MM-DD for created. */
  value: string | number[];
}

export interface JobSort {
  field: FieldKey;
  dir: 'asc' | 'desc';
}

export const STATUS_COLORS = [
  'gray',
  'brown',
  'orange',
  'yellow',
  'green',
  'blue',
  'purple',
  'pink',
  'red',
] as const satisfies readonly StatusColor[];

/** `EngineStatus.state`; a `&'static str` on the Rust side. */
export type EngineState = 'none' | 'missing' | 'idle' | 'ready' | 'error';

export type AppErrorKind = AppError['kind'];

export type ProviderId = 'openrouter' | 'subq' | 'typesafe' | 'exa' | 'localhost';

export type SkipMode = 'cancel' | 'background';
export type ModelRole = 'decision' | 'chat';

// ---- event payloads ----

export type MatchStage =
  | { stage: 'researching'; company_id: number }
  | { stage: 'reading_jd' }
  | { stage: 'scoring'; done: number; total: number }
  | { stage: 'done' }
  | { stage: 'cancelled' }
  | { stage: 'failed'; error: AppError };

export interface MatchProgressEvent {
  run_id: number;
  event: MatchStage;
}

export type ResearchStage =
  | { stage: 'queued' }
  | { stage: 'started' }
  | { stage: 'step'; step: ResearchStep }
  | { stage: 'ready'; company: Company }
  | { stage: 'failed'; error: string }
  | { stage: 'cancelled' };

export interface ResearchEvent {
  company_id: number;
  event: ResearchStage;
}

export interface ScanProgress {
  checked: number;
  total: number;
}

/** Payload of `scan://finished`. */
export interface ScanFinished {
  run_id: number;
  source: ScanSource;
  status: 'done' | 'failed';
  error: string | null;
  error_kind: AppErrorKind | null;
  checked: number;
  job_related: number;
  applied: number;
  pending: number;
  no_change: number;
  failed: number;
}
