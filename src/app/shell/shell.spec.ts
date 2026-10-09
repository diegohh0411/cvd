import { TestBed } from '@angular/core/testing';

import { pageProviders } from '../../testing/providers';
import { flush, mockTauri } from '../../testing/tauri-mock';
import { EngineStore } from '../core/stores/engine.store';
import { ModelsStore } from '../core/stores/models.store';
import { NAV } from './app-sidebar.component';
import { EngineFooterComponent } from './engine-footer.component';

describe('EngineFooterComponent', () => {
  it('shows the active model and follows engine status events', async () => {
    let state = 'idle';
    const t = mockTauri({
      engine_status: () => ({ model_id: 'clef-flash', state, error: null }),
      list_models: [{ spec: { id: 'clef-flash', label: 'Clef-Flash (default)', hf_repo: '', hf_file: '', sha256: '', size_bytes: 1, min_ram_gb: 8 }, installed: true }],
      system_ram_gb: 16,
    });
    TestBed.configureTestingModule({ providers: pageProviders() });
    await TestBed.inject(ModelsStore).load();
    await TestBed.inject(EngineStore).load();
    const f = TestBed.createComponent(EngineFooterComponent);
    f.detectChanges();
    const el = () => (f.nativeElement as HTMLElement).querySelector('[data-testid=engine-footer]')!.textContent!;
    expect(el()).toContain('Clef-Flash · ready');
    expect((f.nativeElement as HTMLElement).querySelector('a[data-testid=engine-footer]')!.getAttribute('href')).toBe('/ai/roles');
    state = 'error';
    await t.emit('engine://status', {});
    await flush();
    f.detectChanges();
    expect(el()).toContain('Clef-Flash · error');
  });
});

describe('sidebar nav', () => {
  it('groups AI pages and keeps Library and Jobs', () => {
    expect(NAV.map((n) => n.title)).toEqual(['Jobs', 'Library', 'AI']);
    expect(NAV[2].items.map((i) => i.link)).toEqual(['/ai/roles', '/ai/models', '/ai/providers']);
  });
});
