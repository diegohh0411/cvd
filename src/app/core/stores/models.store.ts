import { Injectable, signal } from '@angular/core';
import * as api from '../ipc/commands';

@Injectable({ providedIn: 'root' })
export class ModelsStore {
  readonly researchModel = signal<string | null>(null);
  async activate(id: string): Promise<void> { await api.setActiveModel(id); }
  async loadResearchModel(): Promise<void> { this.researchModel.set(await api.getResearchModel().catch(() => null)); }
  async useForResearch(id: string): Promise<void> { await api.setResearchModel(id); this.researchModel.set(id); }
}
