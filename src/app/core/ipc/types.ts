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
  /** Resolved company; null until a match resolves it. */
  company_id: number | null;
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
  /** `researched_at` of the company profile used; null = scored without company context. */
  company_researched_at: string | null;
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
  tags: string[];
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
  /** `decision` = System One; `chat` = company research. */
  kind: ModelKind;
}

export type ModelKind = 'decision' | 'chat';

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

export type AppErrorKind =
  | 'NotFound'
  | 'ModelMissing'
  | 'EngineUnavailable'
  | 'ContextOverflow'
  | 'Render'
  | 'Io'
  | 'ProviderAuth'
  | 'ProviderCredits'
  | 'ProviderUnavailable'
  | 'MailAuth'
  | 'MailUnavailable';

export type ProviderId = 'openrouter' | 'exa';

export interface KeyStatus {
  has_key: boolean;
  last4: string | null;
}

export interface RemoteModel {
  provider: string;
  id: string;
  display_name: string;
  context_length: number | null;
  /** USD per million input tokens. */
  price_per_mtok_in: number | null;
}

export interface ProviderModels {
  provider: string;
  models: RemoteModel[];
  error: string | null;
}

export interface AppError {
  kind: AppErrorKind;
  message: string;
}

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

export interface DownloadEvent {
  id: string;
  done: number;
  total: number;
}

export interface Source {
  url: string;
  title: string;
}

export interface CompanyProfile {
  summary: string;
  industry: string;
  size: string;
  stage: string;
  headquarters: string;
  remote_policy: string;
  products: string;
  tech_stack: string[];
  sources: Source[];
}

export type CompanyStatus = 'pending' | 'ok' | 'failed';

export interface Company {
  id: number;
  name: string;
  normalized_name: string;
  domain: string | null;
  profile: CompanyProfile;
  status: CompanyStatus;
  /** UTC `YYYY-MM-DD HH:MM:SS`. */
  researched_at: string | null;
  model_id: string;
  fail_count: number;
  last_error: string;
  retry_after: string | null;
  edited_at: string | null;
}

export type ResearchStep =
  | { kind: 'searching'; query: string }
  | { kind: 'reading'; domain: string }
  | { kind: 'round'; n: number; max: number };

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

export type SkipMode = 'cancel' | 'background';
export type ModelRole = 'decision' | 'chat';

// ---- Gmail job updates (src-tauri/src/mail, store/email.rs) ----

/** The app password never reaches the webview; `has_password` only says one is stored. */
export interface MailAccountView {
  host: string;
  port: number;
  address: string;
  has_password: boolean;
}

export type ScanOutcome = 'ignored' | 'no_change' | 'applied' | 'pending_review' | 'dismissed';

export interface ProcessedEmail {
  message_id: string;
  run_id: number;
  processed_at: string;
  received_at: string;
  subject: string;
  sender: string;
  matched_job_id: number | null;
  previous_status_id: number | null;
  suggested_status_id: number | null;
  confidence: number | null;
  summary: string | null;
  outcome: ScanOutcome;
  /** Computed by the backend so the webview never builds Gmail links. */
  gmail_url: string;
}

export interface ScanState {
  running: boolean;
}

export interface ScanProgress {
  checked: number;
  total: number;
}

/** Payload of `scan://finished`. */
export interface ScanFinished {
  run_id: number;
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
