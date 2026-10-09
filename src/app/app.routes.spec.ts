import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';

import { ZardSidebarService } from '@/shared/components/sidebar/sidebar.service';
import { flush, mockTauri, type TauriMock } from '../testing/tauri-mock';
import { pageProviders } from '../testing/providers';
import { modelsRedirect, routes } from './app.routes';
import { ProfilePageComponent } from './features/profile/profile-page.component';

const PROFILE = { name: 'Ada', headline: '', email: '', phone: '', location: '', links: [], summary: '' };

describe('routes', () => {
  it('maps old models tabs', () => {
    expect(modelsRedirect('engine')).toBe('/ai/roles');
    expect(modelsRedirect('download')).toBe('/ai/models');
    expect(modelsRedirect(undefined)).toBe('/ai/models');
  });

  it('navigates /ai and /models to the new pages', async () => {
    mockTauri({});
    // Children only, so the shell (and its IPC) is not rendered.
    const children = routes[0].children!.map((r) => ({ ...r, loadComponent: undefined, component: r.redirectTo ? undefined : Blank }));
    TestBed.configureTestingModule({ providers: [provideRouter(children), ZardSidebarService] });
    const h = await RouterTestingHarness.create();
    const router = TestBed.inject(Router);
    const go = async (url: string) => {
      await h.navigateByUrl(url);
      return router.url.split('?')[0];
    };
    expect(await go('/ai')).toBe('/ai/roles');
    expect(await go('/models?tab=engine')).toBe('/ai/roles');
    expect(await go('/models?tab=online')).toBe('/ai/models');
  });
});

@Component({ template: '' })
class Blank {}

describe('ProfilePageComponent', () => {
  let t: TauriMock;

  it('shows the stored profile and saves it', async () => {
    t = mockTauri({ get_profile: PROFILE, save_profile: null });
    TestBed.configureTestingModule({ providers: pageProviders() });
    const f = TestBed.createComponent(ProfilePageComponent);
    f.detectChanges();
    await flush();
    f.detectChanges();
    expect((f.nativeElement as HTMLElement).querySelector<HTMLInputElement>('#p-name')!.value).toBe('Ada');
    const save = [...(f.nativeElement as HTMLElement).querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Save profile')!;
    save.click();
    await flush();
    expect(t.callsTo('save_profile')[0]['profile']).toMatchObject({ name: 'Ada' });
  });
});
