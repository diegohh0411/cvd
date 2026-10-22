import { inject, Injectable } from '@angular/core';
import { platform } from '../../core/platform/runtime';

import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
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
    try {
      const path = await platform.exportCv(runId, selection, suggested);
      if (!path) return null;
      this.toast.success('CV exported', {
        description: path,
        ...(platform.capabilities.revealInFolder ? { action: { label: 'Open', onClick: () => void platform.openFile(path) } } : {}),
      });
      return path;
    } catch (e) {
      this.toast.error('Export failed', { description: messageFor(e) });
      return null;
    }
  }
}
