import { inject, Injectable } from '@angular/core';
import { save } from '@tauri-apps/plugin-dialog';
import { openPath, revealItemInDir } from '@tauri-apps/plugin-opener';

import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import * as api from '../../core/ipc/commands';
import { messageFor } from '../../core/ipc/errors';
import type { JobDescription, Selection } from '../../core/ipc/types';

/** `Name — Company.pdf`, without characters that file systems reject. */
export function defaultFileName(name: string, job: Pick<JobDescription, 'company' | 'title'>): string {
  const base = [name.trim() || 'CV', job.company.trim() || job.title.trim()].filter(Boolean).join(' — ');
  return `${base.replace(/[\\/:*?"<>|]+/g, '-')}.pdf`;
}

/** The backend only accepts absolute `.pdf` paths. */
export function ensurePdf(path: string): string {
  return /\.pdf$/i.test(path) ? path : `${path}.pdf`;
}

@Injectable({ providedIn: 'root' })
export class ExportService {
  private readonly toast = inject(ZardSonnerService);

  /** Asks for a path and exports; resolves to the path, or null if cancelled or failed. */
  async export(runId: number, selection: Selection, suggested: string): Promise<string | null> {
    const picked = await save({ defaultPath: suggested, filters: [{ name: 'PDF', extensions: ['pdf'] }] });
    if (!picked) return null;
    const path = ensurePdf(picked);
    try {
      await api.exportCv(runId, selection, path);
      this.toast.success('CV exported', {
        description: path,
        action: { label: 'Open', onClick: () => void openPath(path).catch(() => revealItemInDir(path)) },
      });
      return path;
    } catch (e) {
      this.toast.error('Export failed', { description: messageFor(e) });
      return null;
    }
  }
}
