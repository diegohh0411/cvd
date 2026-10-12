// One typed wrapper per Tauri command (src-tauri/src/commands.rs `ipc`).
// Argument names are camelCase: Tauri maps them to the snake_case parameters.
import { invoke } from '@tauri-apps/api/core';

import { toAppError } from './errors';
import type {
  Company,
  CompanyProfile,
  Cv,
  Entry,
  EntryInput,
  EngineStatus,
  FitOutput,
  JobDescription,
  JobStatus,
  KeyStatus,
  MailAccountView,
  MatchRun,
  ModelInfo,
  Override,
  Profile,
  ProcessedEmail,
  ProviderId,
  ProviderModels,
  Ranked,
  ScanState,
  ScoreRow,
  Selection,
  Skill,
  SkipMode,
  StatusColor,
  ModelRole,
} from './types';

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toAppError(e);
  }
}

// ---- profile & library ----
export const getProfile = () => call<Profile>('get_profile');
export const saveProfile = (profile: Profile) => call<void>('save_profile', { profile });
export const listEntries = (includeArchived: boolean) => call<Entry[]>('list_entries', { includeArchived });
export const saveEntry = (input: EntryInput) => call<number>('save_entry', { input });
export const deleteEntry = (id: number) => call<void>('delete_entry', { id });
export const listSkills = () => call<Skill[]>('list_skills');

// ---- jobs & matching ----
export const createJob = (title: string, company: string, rawText: string, url = '', statusId: number | null = null) =>
  call<number>('create_job', { title, company, rawText, url, statusId });
export const updateJobTracking = (id: number, statusId: number | null, url: string) =>
  call<void>('update_job_tracking', { id, statusId, url });
export const fetchJobDescription = (url: string) => call<string>('fetch_job_description', { url });
export const listJobs = () => call<JobDescription[]>('list_jobs');
export const getJob = (id: number) => call<JobDescription>('get_job', { id });
export const startMatch = (jdId: number) => call<number>('start_match', { jdId });
export const cancelMatch = (runId: number) => call<void>('cancel_match', { runId });
export const resumeMatch = (runId: number) => call<void>('resume_match', { runId });
export const listRuns = (jdId: number) => call<MatchRun[]>('list_runs', { jdId });
export const getScores = (runId: number) => call<ScoreRow[]>('get_scores', { runId });
export const getRanked = (runId: number) => call<Ranked>('get_ranked', { runId });
export const skipResearch = (runId: number, mode: SkipMode) => call<void>('skip_research', { runId, mode });
export const rescore = (runId: number) => call<number>('rescore', { runId });

// ---- companies ----
export const getCompany = (id: number) => call<Company>('get_company', { id });
export const getJobCompany = (jdId: number) => call<Company | null>('get_job_company', { jdId });
export const editCompany = (id: number, profile: CompanyProfile) => call<Company>('edit_company', { id, profile });
export const refreshCompany = (id: number) => call<Company>('refresh_company', { id });

// ---- job statuses ----
export const listJobStatuses = () => call<JobStatus[]>('list_job_statuses');
export const createJobStatus = (name: string, color: StatusColor) =>
  call<JobStatus>('create_job_status', { name, color });
export const updateJobStatus = (id: number, name: string, color: StatusColor) =>
  call<JobStatus>('update_job_status', { id, name, color });
export const deleteJobStatus = (id: number) => call<void>('delete_job_status', { id });
export const reorderJobStatuses = (ids: number[]) => call<void>('reorder_job_statuses', { ids });

// ---- CV ----
export const fitCv = (runId: number, budget: number, overrides: Override[]) =>
  call<FitOutput>('fit_cv', { runId, budget, overrides });
export const exportCv = (runId: number, selection: Selection, path: string) =>
  call<number>('export_cv', { runId, selection, path });
export const listCvs = (jdId: number) => call<Cv[]>('list_cvs', { jdId });

// ---- settings ----
export const getUiSetting = (key: `ui.${string}`) => call<string | null>('get_ui_setting', { key });
export const setUiSetting = (key: `ui.${string}`, value: string) => call<void>('set_ui_setting', { key, value });

// ---- models & engine ----
export const systemRamGb = () => call<number>('system_ram_gb');
export const listModels = () => call<ModelInfo[]>('list_models');
export const downloadModel = (id: string) => call<void>('download_model', { id });
export const cancelDownload = (id: string) => call<void>('cancel_download', { id });
export const setActiveModel = (id: string) => call<void>('set_active_model', { id });
export const setResearchModel = (id: string) => call<void>('set_research_model', { id });
export const getResearchModel = () => call<string | null>('get_research_model');
export const engineStatus = () => call<EngineStatus>('engine_status');

// ---- online providers ----
export const setProviderKey = (provider: ProviderId, key: string) => call<KeyStatus>('set_provider_key', { provider, key });
export const deleteProviderKey = (provider: ProviderId) => call<void>('delete_provider_key', { provider });
export const providerKeyStatus = (provider: ProviderId) => call<KeyStatus>('provider_key_status', { provider });
export const listRemoteModels = (refresh: boolean, role: ModelRole = 'decision') =>
  call<ProviderModels[]>('list_remote_models', { refresh, role });

// ---- Gmail job updates ----
export const getMailAccount = () => call<MailAccountView | null>('get_mail_account');
/** `password: null` keeps the stored app password. */
export const saveMailAccount = (host: string, port: number, address: string, password: string | null, folder: string | null) =>
  call<MailAccountView>('save_mail_account', { host, port, address, password, folder });
export const deleteMailAccount = () => call<void>('delete_mail_account');
/** Logs in and returns the mailboxes the account can open. */
export const testMailAccount = (host: string, port: number, address: string, password: string | null) =>
  call<string[]>('test_mail_account', { host, port, address, password });
export const startScan = () => call<number>('start_scan');
export const getScanStatus = () => call<ScanState>('get_scan_status');
export const listScanResults = (runId: number) => call<ProcessedEmail[]>('list_scan_results', { runId });
export const listPendingReviews = () => call<ProcessedEmail[]>('list_pending_reviews');
export const applySuggestion = (messageId: string) => call<ProcessedEmail>('apply_suggestion', { messageId });
export const dismissSuggestion = (messageId: string) => call<void>('dismiss_suggestion', { messageId });
