import { computed, Injectable, signal } from '@angular/core';

import * as api from '../ipc/commands';
import { messageFor } from '../ipc/errors';
import { onDownload } from '../ipc/events';
import type { ModelInfo, ModelSpec } from '../ipc/types';

export const DEFAULT_MODEL = 'clef-flash';
export const DOWNLOAD_PAUSED = 'Download paused.';

export interface DownloadState {
  done: number;
  total: number;
  /** Bytes per second, smoothed. */
  rate: number;
  status: 'downloading' | 'paused' | 'error' | 'done';
  error?: string;
  at: number;
}

@Injectable({ providedIn: 'root' })
export class ModelsStore {
  readonly models = signal<ModelInfo[]>([]);
  readonly ramGb = signal<number | null>(null);
  readonly downloads = signal<Record<string, DownloadState | undefined>>({});
  readonly error = signal<string | null>(null);
  private listening = false;

  readonly installed = computed(() => this.models().filter((m) => m.installed));
  /** Largest supported model, preferring the default when it fits. */
  readonly recommended = computed<ModelSpec | null>(() => recommend(this.models().map((m) => m.spec), this.ramGb()));

  async load(): Promise<void> {
    this.listen();
    try {
      const [models, ram] = await Promise.all([api.listModels(), api.systemRamGb()]);
      this.models.set(models);
      this.ramGb.set(ram);
      this.error.set(null);
    } catch (e) {
      this.error.set(messageFor(e));
    }
  }

  supports(spec: ModelSpec): boolean {
    const ram = this.ramGb();
    return ram === null || ram >= spec.min_ram_gb;
  }

  /** Starts or resumes a download (the backend resumes partial files). */
  async download(id: string): Promise<boolean> {
    this.listen();
    this.patch(id, { status: 'downloading', error: undefined });
    try {
      await api.downloadModel(id);
      this.patch(id, { status: 'done' });
      await this.load();
      return true;
    } catch (e) {
      const message = messageFor(e);
      this.patch(id, message === DOWNLOAD_PAUSED ? { status: 'paused' } : { status: 'error', error: message });
      return false;
    }
  }

  async pause(id: string): Promise<void> {
    await api.cancelDownload(id);
  }

  async activate(id: string): Promise<void> {
    await api.setActiveModel(id);
  }

  readonly researchModel = signal<string | null>(null);

  async loadResearchModel(): Promise<void> {
    this.researchModel.set(await api.getResearchModel().catch(() => null));
  }

  async useForResearch(id: string): Promise<void> {
    await api.setResearchModel(id);
    this.researchModel.set(id);
  }

  private listen(): void {
    if (this.listening) return;
    this.listening = true;
    void onDownload(({ id, done, total }) => {
      const prev = this.downloads()[id];
      const now = Date.now();
      let rate = prev?.rate ?? 0;
      if (prev && now > prev.at && done >= prev.done) {
        const inst = ((done - prev.done) * 1000) / (now - prev.at);
        rate = rate ? rate * 0.8 + inst * 0.2 : inst;
      }
      this.patch(id, { done, total, rate, at: now, status: 'downloading' });
    });
  }

  private patch(id: string, p: Partial<DownloadState>): void {
    this.downloads.update((d) => {
      const prev: DownloadState = d[id] ?? { done: 0, total: 0, rate: 0, status: 'downloading', at: Date.now() };
      return { ...d, [id]: { ...prev, ...p } };
    });
  }
}

/** Recommends a System One (decision) model; research models are chosen on AI › Roles. */
export function recommend(specs: ModelSpec[], ramGb: number | null): ModelSpec | null {
  const fits = specs.filter((s) => s.kind === 'decision' && (ramGb === null || s.min_ram_gb <= ramGb));
  return fits.find((s) => s.id === DEFAULT_MODEL) ?? [...fits].sort((a, b) => b.size_bytes - a.size_bytes)[0] ?? null;
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ['KB', 'MB', 'GB', 'TB'];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 10 ? 0 : 1)} ${units[i]}`;
}
