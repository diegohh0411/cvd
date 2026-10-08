// One typed wrapper per Tauri command (src-tauri/src/commands.rs `ipc`).
// Argument names are camelCase: Tauri maps them to the snake_case parameters.
import { invoke } from '@tauri-apps/api/core';

import { toAppError } from './errors';
import type {
  Cv,
  Entry,
  EntryInput,
  EngineStatus,
  FitOutput,
  JobDescription,
  MatchRun,
  ModelInfo,
  Override,
  Profile,
  Ranked,
  ScoreRow,
  Selection,
  Skill,
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
export const createJob = (title: string, company: string, rawText: string) =>
  call<number>('create_job', { title, company, rawText });
export const listJobs = () => call<JobDescription[]>('list_jobs');
export const getJob = (id: number) => call<JobDescription>('get_job', { id });
export const startMatch = (jdId: number) => call<number>('start_match', { jdId });
export const cancelMatch = (runId: number) => call<void>('cancel_match', { runId });
export const resumeMatch = (runId: number) => call<void>('resume_match', { runId });
export const listRuns = (jdId: number) => call<MatchRun[]>('list_runs', { jdId });
export const getScores = (runId: number) => call<ScoreRow[]>('get_scores', { runId });
export const getRanked = (runId: number) => call<Ranked>('get_ranked', { runId });

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
export const engineStatus = () => call<EngineStatus>('engine_status');
