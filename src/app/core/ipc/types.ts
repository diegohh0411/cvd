// Mirrors of the Rust types the webview sees (src-tauri/src/**). Field names
// stay snake_case because serde serializes them as-is.

export interface Profile {
  name: string;
  headline: string;
  email: string;
  phone: string;
  location: string;
  links: string[];
  summary: string;
}

export type EntryKind = 'experience' | 'project' | 'education' | 'volunteer';
export const ENTRY_KINDS: readonly EntryKind[] = ['experience', 'project', 'education', 'volunteer'];

export interface Bullet {
  id: number;
  entry_id: number;
  text: string;
  position: number;
}

export interface Skill {
  id: number;
  name: string;
  category: string;
}

export interface Entry {
  id: number;
  kind: EntryKind;
  title: string;
  org: string;
  location: string;
  /** `YYYY-MM` */
  start_date: string;
  end_date: string | null;
  description: string;
  sort_hint: number;
  archived: boolean;
  bullets: Bullet[];
  skills: Skill[];
}

/** `id: null` inserts. Bullets are texts in display order; skills are names. */
export interface EntryInput {
  id: number | null;
  kind: EntryKind;
  title: string;
  org: string;
  location: string;
  start_date: string;
  end_date: string | null;
  description: string;
  sort_hint: number;
  archived: boolean;
  bullets: string[];
  skills: string[];
}

export interface JobDescription {
  id: number;
  title: string;
  company: string;
  raw_text: string;
  created_at: string;
  /** Link to the original posting; '' when unknown. */
  url: string;
  status_id: number | null;
}

export const STATUS_COLORS = ['gray', 'brown', 'orange', 'yellow', 'green', 'blue', 'purple', 'pink', 'red'] as const;
export type StatusColor = (typeof STATUS_COLORS)[number];

export interface JobStatus {
  id: number;
  name: string;
  color: StatusColor;
  position: number;
}

export type RunStatus = 'running' | 'done' | 'cancelled' | 'failed';

export interface MatchRun {
  id: number;
  jd_id: number;
  model_id: string;
  status: RunStatus;
  created_at: string;
}

export type TargetType = 'entry' | 'bullet' | 'skill' | 'meta';

export interface ScoreRow {
  run_id: number;
  target_type: TargetType;
  target_id: number;
  question: string;
  probabilities: Record<string, number>;
  score: number;
}

export interface RankedEntry {
  entry: Entry;
  score: number;
  bullets: [Bullet, number][];
}

export interface Ranked {
  entries: RankedEntry[];
  skills: [Skill, number][];
  seniority: string;
}

export interface SelectedEntry {
  entry_id: number;
  /** In stored position order. */
  bullet_ids: number[];
  pinned: boolean;
}

export interface Selection {
  entries: SelectedEntry[];
  skills: string[];
}

/** Serde's externally tagged enum: `{ Entry: 5 }` / `{ Bullet: 9 }`. */
export type Target = { Entry: number } | { Bullet: number };

/** `{ Include: Target }`, `{ Exclude: Target }` or `{ Order: entryIds }`. */
export type Override = { Include: Target } | { Exclude: Target } | { Order: number[] };

export interface FitOutput {
  selection: Selection;
  pages: number;
  fill: number;
  overflow: boolean;
  pdf_base64: string;
}

export interface Cv {
  id: number;
  jd_id: number;
  run_id: number | null;
  template: string;
  page_budget: number;
  selection_json: string;
  pdf_path: string | null;
  created_at: string;
}

export interface ModelSpec {
  id: string;
  label: string;
  hf_repo: string;
  hf_file: string;
  sha256: string;
  size_bytes: number;
  min_ram_gb: number;
}

export interface ModelInfo {
  spec: ModelSpec;
  installed: boolean;
}

export type EngineState = 'none' | 'missing' | 'idle' | 'ready' | 'error';

export interface EngineStatus {
  model_id: string | null;
  state: EngineState;
  error: string | null;
}

export type AppErrorKind = 'NotFound' | 'ModelMissing' | 'EngineUnavailable' | 'ContextOverflow' | 'Render' | 'Io';

export interface AppError {
  kind: AppErrorKind;
  message: string;
}

export type MatchStage =
  | { stage: 'reading_jd' }
  | { stage: 'scoring_entries'; done: number; total: number }
  | { stage: 'picking_bullets'; done: number; total: number }
  | { stage: 'done' }
  | { stage: 'cancelled' }
  | { stage: 'failed'; error: AppError };

export interface MatchProgressEvent {
  run_id: number;
  event: MatchStage;
}

export interface DownloadEvent {
  id: string;
  done: number;
  total: number;
}
