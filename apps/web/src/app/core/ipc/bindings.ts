// Generated from the Rust types by crates/cvd-core/src/bindings.rs; do not edit.
// Regenerate: CVD_UPDATE_BINDINGS=1 cargo test -p cvd-core bindings
import type { AppErrorKind, EngineState, JobFilter, JobSort, ProviderId } from './types';

export type Profile = { name: string, headline: string, email: string, phone: string, location: string, links: Array<string>, summary: string, };

export type ApplyDefaults = { 
/**
 * Overrides for the split of `Profile.name`.
 */
first_name: string, last_name: string, city: string, region: string, country: string, authorized_to_work: boolean | null, needs_sponsorship: boolean | null, willing_to_relocate: boolean | null, notice_period: string, earliest_start: string, salary_expectation: string, };

export type ApplyFact = { key: string, 
/**
 * Non-empty, deduplicated, preferred spelling first.
 */
variants: Array<string>, };

export type EntryKind = "experience" | "project" | "education" | "volunteer";

export type Bullet = { id: number, entry_id: number, text: string, position: number, };

export type Skill = { id: number, name: string, category: string, };

export type Entry = { id: number, kind: EntryKind, title: string, org: string, location: string, 
/**
 * `YYYY-MM`
 */
start_date: string, end_date: string | null, description: string, sort_hint: number, archived: boolean, bullets: Array<Bullet>, skills: Array<Skill>, };

export type EntryInput = { id: number | null, kind: EntryKind, title: string, org: string, location: string, start_date: string, end_date: string | null, description: string, sort_hint: number, archived: boolean, 
/**
 * Bullet texts in display order.
 */
bullets: Array<string>, 
/**
 * Skill names; created on demand (case-insensitive).
 */
skills: Array<string>, };

export type JobDescription = { id: number, title: string, company: string, raw_text: string, created_at: string, 
/**
 * Link to the original posting; empty when unknown.
 */
url: string, 
/**
 * Application status (see `JobStatus`); null when unset.
 */
status_id: number | null, 
/**
 * Resolved company (see `Company`); null until a match resolves it.
 */
company_id: number | null, };

export type ViewLayout = "table" | "board";

export type JobView = { id: number, name: string, layout: ViewLayout, filters: JobFilter[], sorts: JobSort[], position: number, };

export type JobViewInput = { name: string, layout: ViewLayout, filters: JobFilter[], sorts: JobSort[], };

export type StatusColor = "gray" | "brown" | "orange" | "yellow" | "green" | "blue" | "purple" | "pink" | "red";

export type JobStatus = { id: number, name: string, color: StatusColor, position: number, };

export type RunStatus = "running" | "done" | "cancelled" | "failed";

export type MatchRun = { id: number, jd_id: number, model_id: string, status: RunStatus, created_at: string, 
/**
 * `researched_at` of the company profile the run was scored with; null = no context.
 */
company_researched_at: string | null, };

export type TargetType = "entry" | "bullet" | "skill" | "meta";

export type ScoreRow = { run_id: number, target_type: TargetType, target_id: number, question: string, probabilities: { [key in string]: number }, score: number, };

export type RankedEntry = { entry: Entry, score: number, bullets: Array<[Bullet, number]>, };

export type Ranked = { entries: Array<RankedEntry>, skills: Array<[Skill, number]>, seniority: string, 
/**
 * JD focus tags the model judged relevant, most probable first.
 */
tags: Array<string>, };

export type SelectedEntry = { entry_id: number, 
/**
 * In stored position order.
 */
bullet_ids: Array<number>, pinned: boolean, };

export type Selection = { entries: Array<SelectedEntry>, skills: Array<string>, };

export type Target = { "Entry": number } | { "Bullet": number };

export type Override = { "Include": Target } | { "Exclude": Target } | { "Order": Array<number> } | { "BulletOrder": Array<number> };

export type FitOutput = { selection: Selection, pages: number, fill: number, overflow: boolean, pdf_base64: string, };

export type Cv = { id: number, jd_id: number, run_id: number | null, template: string, page_budget: number, selection_json: string, file_ref: string | null, created_at: string, };

export type EngineStatus = { model_id: string | null, 
/**
 * `none` (no model), `idle` (configured), `ready` (engine built), or `error`.
 */
state: EngineState, error: string | null, };

export type AppError = { "kind": "NotFound", "message": string } | { "kind": "ModelMissing", "message": string } | { "kind": "EngineUnavailable", "message": string } | { "kind": "ContextOverflow", "message": string } | { "kind": "Render", "message": string } | { "kind": "Io", "message": string } | { "kind": "ProviderAuth", "message": string } | { "kind": "ProviderCredits", "message": string } | { "kind": "ProviderUnavailable", "message": string } | { "kind": "MailAuth", "message": string } | { "kind": "MailUnavailable", "message": string };

export type KeyStatus = { has_key: boolean, last4: string | null, };

export type RemoteModel = { provider: string, id: string, display_name: string, context_length: number | null, 
/**
 * USD per million input tokens.
 */
price_per_mtok_in: number | null, };

export type Connection = { id: string, provider: ProviderId, label: string, last4: string | null, };

export type ProviderModels = { 
/**
 * The connection id (`openrouter`, `openrouter~2`): what model refs carry.
 */
provider: string, label: string, models: Array<RemoteModel>, error: string | null, };

export type LocalConnectionView = { base_url: string, has_key: boolean, };

export type Source = { url: string, title: string, };

export type CompanyProfile = { summary: string, industry: string, size: string, stage: string, headquarters: string, remote_policy: string, products: string, tech_stack: Array<string>, sources: Array<Source>, };

export type CompanyStatus = "pending" | "ok" | "failed";

export type Company = { id: number, name: string, normalized_name: string, domain: string | null, profile: CompanyProfile, status: CompanyStatus, researched_at: string | null, model_id: string, fail_count: number, last_error: string, retry_after: string | null, edited_at: string | null, };

export type ResearchStep = { "kind": "searching", query: string, } | { "kind": "reading", domain: string, } | { "kind": "round", n: number, max: number, };

export type MailAccountView = { host: string, port: number, address: string, has_password: boolean, 
/**
 * The mailbox the scan reads.
 */
folder: string, };

export type ScanOutcome = "ignored" | "no_change" | "applied" | "pending_review" | "dismissed";

export type ProcessedEmailRow = { message_id: string, run_id: number, processed_at: string, received_at: string, subject: string, sender: string, matched_job_id: number | null, previous_status_id: number | null, suggested_status_id: number | null, confidence: number | null, summary: string | null, outcome: ScanOutcome, job_related: number | null, body: string | null, 
/**
 * `None` until the row is read in the inbox.
 */
read_at: string | null, 
/**
 * Set when an auto-applied status was undone.
 */
undone_at: string | null, };

export type ProcessedEmail = { gmail_url: string, message_id: string, run_id: number, processed_at: string, received_at: string, subject: string, sender: string, matched_job_id: number | null, previous_status_id: number | null, suggested_status_id: number | null, confidence: number | null, summary: string | null, outcome: ScanOutcome, job_related: number | null, body: string | null, 
/**
 * `None` until the row is read in the inbox.
 */
read_at: string | null, 
/**
 * Set when an auto-applied status was undone.
 */
undone_at: string | null, };

export type InboxFilter = "all" | "review" | "unread";

export type WatchState = "connecting" | "watching" | "scanning" | "paused" | "stopped";

export type WatcherStatus = { state: WatchState, error: string | null, error_kind: AppErrorKind | null, 
/**
 * RFC 3339 time of the last successful cycle.
 */
last_checked_at: string | null, };

export type ScanSource = "watch" | "recheck" | "manual";

export type ScanState = { running: boolean, };
