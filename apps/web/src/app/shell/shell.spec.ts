import { TestBed } from '@angular/core/testing';

import { pageProviders } from '../../testing/providers';
import { flush, mockTauri } from '../../testing/tauri-mock';
import { EngineStore } from '../core/stores/engine.store';
import { InboxStore } from '../core/stores/inbox.store';
import { AppSidebarComponent, NAV } from './app-sidebar.component';
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

describe('sidebar inbox link', () => {
  async function render(watcher: Record<string, unknown>, unread: number) {
    mockTauri({
      list_jobs: [],
      get_profile: null,
      engine_status: { model_id: null, state: 'none', error: null },
      list_inbox: [],
      unread_count: unread,
      get_watcher_status: watcher,
    });
    TestBed.configureTestingModule({ providers: pageProviders() });
    const f = TestBed.createComponent(AppSidebarComponent);
    f.detectChanges();
    await TestBed.inject(InboxStore).ensureReady();
    await flush();
    f.detectChanges();
    return (f.nativeElement as HTMLElement).querySelector('a[href="/inbox"]') as HTMLElement;
  }

  it('sidebar shows unread badge', async () => {
    const link = await render({ state: 'watching', error: null, error_kind: null, last_checked_at: null }, 3);

    expect(link.textContent).toContain('Inbox');
    expect(link.querySelector('[data-testid="unread-badge"]')?.textContent?.trim()).toBe('3');
  });

  it('sidebar shows paused dot', async () => {
    const link = await render({ state: 'paused', error: 'Bad password', error_kind: 'MailAuth', last_checked_at: null }, 0);

    expect(link.querySelector('[aria-label="Mail watcher paused"]')).toBeTruthy();
    expect(link.querySelector('[data-testid="unread-badge"]')).toBeNull();
  });
});
