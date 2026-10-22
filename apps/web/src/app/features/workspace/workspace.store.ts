import { computed, DestroyRef, inject, Injectable, signal } from '@angular/core';
import type { UnlistenFn } from '@tauri-apps/api/event';

import * as api from '../../core/ipc/commands';
import { toAppError } from '../../core/ipc/errors';
import { onAnyMatchProgress, onResearch, onTablesChanged, untilDestroyed } from '../../core/ipc/events';
import { stepLine } from '../companies/research-progress.component';
import type {
  AppError,
  Company,
  ResearchEvent,
  SkipMode,
  FitOutput,
  JobDescription,
  MatchProgressEvent,
  MatchRun,
  MatchStage,
  Ranked,
  ScoreRow,
  Selection,
} from '../../core/ipc/types';
import { JobsStore } from '../../core/stores/jobs.store';
import {
  buildView,
  counts,
  NO_OVERRIDES,
  type OverrideState,
  reorder,
  reorderBullets,
  toggleBullet,
  freeze,
  toggleEntry,
  toOverrides,
} from './selection.model';

/** Live company research while a run waits on it. */
export interface ResearchState {
  companyId: number;
  round: { n: number; max: number } | null;
  /** Human-readable agent steps, oldest first. */
  log: string[];
  startedAt: number;
}

/** Steps kept in the live log. */
const LOG_LINES = 6;

export type Phase = 'loading' | 'new' | 'matching' | 'review' | 'cancelled' | 'failed';

export const FIT_DEBOUNCE_MS = 150;
export const BUDGET_SETTING = 'ui.page_budget' as const;

/** Decodes the fitter's base64 PDF once, off the template path. */
export function decodeBase64(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** One job's workspace; provided per route so state resets between jobs. */
@Injectable()
export class WorkspaceStore {
  private readonly jobs = inject(JobsStore);

  readonly job = signal<JobDescription | null>(null);
  readonly run = signal<MatchRun | null>(null);
  readonly loadError = signal<AppError | null>(null);
  readonly progress = signal<MatchStage | null>(null);
  readonly runError = signal<AppError | null>(null);
  readonly startedAt = signal<number | null>(null);
  readonly finishedAt = signal<number | null>(null);
  readonly research = signal<ResearchState | null>(null);
  /** The job's company profile, once resolved. */
  readonly company = signal<Company | null>(null);

  readonly ranked = signal<Ranked | null>(null);
  readonly scores = signal<ScoreRow[]>([]);
  readonly budget = signal(1);
  readonly overrides = signal<OverrideState>(NO_OVERRIDES);
  readonly fit = signal<Omit<FitOutput, 'pdf_base64'> | null>(null);
  /** Last good PDF; kept when a later compile fails (spec §8). */
  readonly pdf = signal<Uint8Array | null>(null);
  readonly fitError = signal<AppError | null>(null);
  readonly fitting = signal(false);
  /** Set once the user changes anything, so Re-match can confirm. */
  readonly edited = signal(false);

  private fitSeq = 0;
  private fitTimer: ReturnType<typeof setTimeout> | null = null;
  private unlisten: Promise<UnlistenFn> | null = null;
  private unlistenResearch: Promise<UnlistenFn> | null = null;
  /** Progress that arrived before `start_match` returned its run id. */
  private early = new Map<number, MatchStage[]>();
  private readonly awaitingRun = signal(false);

  readonly phase = computed<Phase>(() => {
    if (!this.job()) return 'loading';
    const run = this.run();
    if (!run) return this.awaitingRun() ? 'matching' : 'new';
    switch (run.status) {
      case 'running':
        return 'matching';
      case 'done':
        return 'review';
      default:
        return run.status;
    }
  });

  readonly selection = computed<Selection | null>(() => this.fit()?.selection ?? null);
  readonly view = computed(() => (this.ranked() ? buildView(this.ranked()!, this.selection(), this.overrides()) : []));
  readonly counts = computed(() => (this.ranked() ? counts(this.ranked()!, this.selection()) : null));
  readonly durationMs = computed(() => {
    const s = this.startedAt();
    const f = this.finishedAt();
    return s && f ? f - s : null;
  });

  constructor() {
    const destroyRef = inject(DestroyRef);
    void untilDestroyed(destroyRef, onTablesChanged(['job_description'], () => void this.reloadJob())).catch(() => undefined);
    void untilDestroyed(destroyRef, onTablesChanged(['company', 'company_alias'], () => void this.loadCompany())).catch(() => undefined);
    destroyRef.onDestroy(() => {
      if (this.fitTimer) clearTimeout(this.fitTimer);
      void this.unlisten?.then((u) => u());
      void this.unlistenResearch?.then((u) => u());
    });
  }

  async init(jobId: number, autoStart: boolean): Promise<void> {
    this.listen();
    try {
      const [job, runs, budget] = await Promise.all([
        api.getJob(jobId),
        api.listRuns(jobId),
        api.getUiSetting(BUDGET_SETTING).catch(() => null),
      ]);
      this.budget.set(budget === '2' ? 2 : 1);
      this.run.set(runs[0] ?? null);
      this.job.set(job);
      void this.loadCompany();
      if (runs[0]?.status === 'done') await this.loadResults();
      else if (!runs[0] && autoStart) await this.start();
    } catch (e) {
      this.loadError.set(toAppError(e));
    }
  }

  /** Subscribes before any run starts so no progress is lost (Review Focus #2). */
  private listen(): void {
    this.unlisten ??= onAnyMatchProgress((e) => this.onProgress(e));
    this.unlistenResearch ??= onResearch((e) => this.onResearch(e));
  }

  private onResearch({ company_id, event }: ResearchEvent): void {
    const r = this.research();
    if (!r || r.companyId !== company_id) return;
    if (event.stage === 'ready') this.company.set(event.company);
    if (event.stage !== 'step') return;
    if (event.step.kind === 'round') {
      this.research.set({ ...r, round: { n: event.step.n, max: event.step.max } });
    } else {
      this.research.set({ ...r, log: [...r.log, stepLine(event.step)].slice(-LOG_LINES) });
    }
  }

  /** Picks up job edits made elsewhere (auto-apply, the jobs list, the MCP server). */
  private async reloadJob(): Promise<void> {
    const job = this.job();
    if (!job) return;
    const fresh = await api.getJob(job.id).catch(() => null);
    if (fresh && this.job()?.id === fresh.id) this.job.set(fresh);
  }

  async loadCompany(): Promise<void> {
    const job = this.job();
    if (!job) return;
    this.company.set(await api.getJobCompany(job.id).catch(() => null));
  }

  private onProgress({ run_id, event }: MatchProgressEvent): void {
    const run = this.run();
    if (!run || run.id !== run_id) {
      if (this.awaitingRun()) this.early.set(run_id, [...(this.early.get(run_id) ?? []), event]);
      return;
    }
    this.apply(event);
  }

  private apply(event: MatchStage): void {
    const run = this.run();
    if (!run) return;
    if (event.stage === 'researching') {
      this.research.set({ companyId: event.company_id, round: null, log: [], startedAt: Date.now() });
    } else {
      this.research.set(null);
    }
    switch (event.stage) {
      case 'done':
        this.progress.set(event);
        this.finishedAt.set(Date.now());
        this.setRun({ ...run, status: 'done' });
        void this.afterDone(run.id);
        break;
      case 'cancelled':
        this.setRun({ ...run, status: 'cancelled' });
        break;
      case 'failed':
        this.runError.set(event.error);
        this.setRun({ ...run, status: 'failed' });
        break;
      default:
        this.progress.set(event);
    }
  }

  private setRun(run: MatchRun): void {
    this.run.set(run);
    this.jobs.setLatestRun(run);
  }

  async start(): Promise<void> {
    const job = this.job();
    if (!job) return;
    this.resetRun();
    this.awaitingRun.set(true);
    this.run.set(null);
    this.startedAt.set(Date.now());
    this.progress.set({ stage: 'reading_jd' });
    try {
      const runId = await api.startMatch(job.id);
      this.setRun({ id: runId, jd_id: job.id, model_id: '', status: 'running', created_at: '', company_researched_at: null });
      this.awaitingRun.set(false);
      for (const e of this.early.get(runId) ?? []) this.apply(e);
      this.early.clear();
    } catch (e) {
      this.awaitingRun.set(false);
      this.progress.set(null);
      this.run.set(null);
      throw toAppError(e);
    }
  }

  /** Reloads the finished run (for `company_researched_at`), the company and the results. */
  private async afterDone(runId: number): Promise<void> {
    const job = this.job();
    if (job) {
      const fresh = (await api.listRuns(job.id).catch(() => [])).find((r) => r.id === runId);
      if (fresh && this.run()?.id === runId) this.setRun(fresh);
    }
    await Promise.all([this.loadCompany(), this.loadResults()]);
  }

  /** Stops waiting on company research; `cancel` also stops the research itself. */
  async skipResearch(mode: SkipMode): Promise<void> {
    const run = this.run();
    if (run) await api.skipResearch(run.id, mode);
  }

  /** Scores the job again in a new run, with the company profile as it is now. */
  async rescore(): Promise<void> {
    const run = this.run();
    const job = this.job();
    if (!run || !job) return;
    this.resetRun();
    this.startedAt.set(Date.now());
    this.progress.set({ stage: 'reading_jd' });
    this.awaitingRun.set(true);
    try {
      const id = await api.rescore(run.id);
      this.setRun({ id, jd_id: job.id, model_id: '', status: 'running', created_at: '', company_researched_at: null });
      for (const e of this.early.get(id) ?? []) this.apply(e);
      this.early.clear();
    } finally {
      this.awaitingRun.set(false);
    }
  }

  async cancel(): Promise<void> {
    const run = this.run();
    if (run) await api.cancelMatch(run.id);
  }

  private resetRun(): void {
    this.runError.set(null);
    this.research.set(null);
    this.ranked.set(null);
    this.scores.set([]);
    this.fit.set(null);
    this.pdf.set(null);
    this.fitError.set(null);
    this.overrides.set(NO_OVERRIDES);
    this.edited.set(false);
    this.finishedAt.set(null);
  }

  async loadResults(): Promise<void> {
    const run = this.run();
    if (!run) return;
    const [ranked, scores] = await Promise.all([api.getRanked(run.id), api.getScores(run.id)]);
    this.ranked.set(ranked);
    this.scores.set(scores);
    const saved = await this.loadSaved(run.id);
    if (saved) {
      this.overrides.set(saved);
      this.edited.set(true);
    }
    await this.refitNow();
  }

  // ---- review edits ----

  toggleEntry(entryId: number, include: boolean): void {
    const r = this.ranked();
    if (!r) return;
    this.edit(toggleEntry(this.base(), r, entryId, include));
  }

  toggleBullet(bulletId: number, include: boolean): void {
    const r = this.ranked();
    if (!r) return;
    this.edit(toggleBullet(this.base(), r, bulletId, include));
  }

  reorder(entryIds: number[]): void {
    this.edit(reorder(this.base(), entryIds));
  }

  reorderBullets(entryId: number, from: number, to: number): void {
    const next = reorderBullets(this.base(), this.view(), entryId, from, to);
    if (next) this.edit(next);
  }

  setBudget(budget: number): void {
    this.budget.set(budget);
    void api.setUiSetting(BUDGET_SETTING, String(budget)).catch(() => undefined);
    this.refit();
  }

  /** Replaces overrides with a saved selection (Exports › Restore). */
  restore(selection: Selection): void {
    const r = this.ranked();
    if (!r) return;
    this.edit(freeze(r, selection, undefined, selection.entries.flatMap((e) => e.bullet_ids)));
  }

  /** Overrides to edit from: the matcher's pick is frozen on the first edit so the fitter never re-adds bullets. */
  private base(): OverrideState {
    const r = this.ranked();
    const sel = this.selection();
    return !this.edited() && r && sel ? freeze(r, sel, this.overrides().order, this.overrides().bulletOrder) : this.overrides();
  }

  /** Overrides survive leaving the job: the store is route-scoped, so they are kept per run in UI settings. */
  private async loadSaved(runId: number): Promise<OverrideState | null> {
    try {
      const raw = await api.getUiSetting(`ui.overrides.${runId}`);
      const parsed = raw ? (JSON.parse(raw) as OverrideState) : null;
      return parsed && typeof parsed.targets === 'object' ? parsed : null;
    } catch {
      return null;
    }
  }

  private persist(): void {
    const run = this.run();
    if (run) void api.setUiSetting(`ui.overrides.${run.id}`, JSON.stringify(this.overrides())).catch(() => undefined);
  }

  private edit(next: OverrideState): void {
    this.overrides.set(next);
    this.edited.set(true);
    this.persist();
    this.refit();
  }

  /** Debounced; only the newest request's answer is applied (Review Focus #1). */
  refit(): void {
    if (this.fitTimer) clearTimeout(this.fitTimer);
    this.fitTimer = setTimeout(() => void this.refitNow(), FIT_DEBOUNCE_MS);
  }

  async refitNow(): Promise<void> {
    const run = this.run();
    if (!run || run.status !== 'done') return;
    const seq = ++this.fitSeq;
    this.fitting.set(true);
    try {
      const out = await api.fitCv(run.id, this.budget(), toOverrides(this.overrides()));
      if (seq !== this.fitSeq) return;
      const { pdf_base64, ...rest } = out;
      this.fit.set(rest);
      this.pdf.set(decodeBase64(pdf_base64));
      this.fitError.set(null);
    } catch (e) {
      if (seq === this.fitSeq) this.fitError.set(toAppError(e));
    } finally {
      if (seq === this.fitSeq) this.fitting.set(false);
    }
  }
}
