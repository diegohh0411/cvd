import { TestBed } from '@angular/core/testing';

import { pageProviders } from '../../testing/providers';
import { flush, mockTauri } from '../../testing/tauri-mock';
import { EngineStore } from '../core/stores/engine.store';
import { NAV } from './app-sidebar.component';
import { EngineFooterComponent } from './engine-footer.component';

describe('EngineFooterComponent', () => {
  it('shows the active model and follows engine status events', async () => {
    let state = 'idle';
    const t = mockTauri({
      engine_status: () => ({ model_id: 'localhost:Clef-Flash', state, error: null }),
    });
    TestBed.configureTestingModule({ providers: pageProviders() });
    await TestBed.inject(EngineStore).load();
    const f = TestBed.createComponent(EngineFooterComponent);
    f.detectChanges();
    const el = () => (f.nativeElement as HTMLElement).querySelector('[data-testid=engine-footer]')!.textContent!;
    expect(el()).toContain('Clef-Flash (local) · ready');
    expect((f.nativeElement as HTMLElement).querySelector('a[data-testid=engine-footer]')!.getAttribute('href')).toBe('/ai');
    state = 'error';
    await t.emit('engine://status', {});
    await flush();
    f.detectChanges();
    expect(el()).toContain('Clef-Flash (local) · error');
  });
});

describe('sidebar nav', () => {
  it('groups AI pages and keeps Library and Jobs', () => {
    expect(NAV.map((n) => n.title)).toEqual(['Jobs', 'Library', 'AI']);
    expect(NAV[2].items.map((i) => i.link)).toEqual(['/ai']);
  });
});
