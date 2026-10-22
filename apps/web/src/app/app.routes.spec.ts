import { Component } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { provideRouter, Router } from '@angular/router';
import { RouterTestingHarness } from '@angular/router/testing';

import { ZardSidebarService } from '@/shared/components/sidebar/sidebar.service';
import { flush, mockTauri, type TauriMock } from '../testing/tauri-mock';
import { pageProviders } from '../testing/providers';
import { modelsRedirect, routes } from './app.routes';
import { EMPTY_DEFAULTS, flagToValue, valueToFlag } from './features/profile/apply-defaults.component';
import { ProfilePageComponent } from './features/profile/profile-page.component';

const PROFILE = { name: 'Ada', headline: '', email: '', phone: '', location: '', links: [], summary: '' };

describe('routes', () => {
  it('maps old models tabs', () => {
    expect(modelsRedirect('engine')).toBe('/ai');
    expect(modelsRedirect('download')).toBe('/ai');
    expect(modelsRedirect(undefined)).toBe('/ai');
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
    expect(await go('/ai')).toBe('/ai');
    expect(await go('/ai/roles')).toBe('/ai');
    expect(await go('/ai/providers')).toBe('/ai');
    expect(await go('/ai/models')).toBe('/ai');
    expect(await go('/models?tab=engine')).toBe('/ai');
    expect(await go('/models?tab=online')).toBe('/ai');
    expect(await go('/inbox')).toBe('/inbox');
  });
});

@Component({ template: '' })
class Blank {}

describe('ProfilePageComponent', () => {
  let t: TauriMock;

  it('shows the stored profile and saves it', async () => {
    t = mockTauri({ get_profile: PROFILE, save_profile: null, get_apply_defaults: EMPTY_DEFAULTS, apply_facts: [] });
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

describe('ApplyDefaultsComponent', () => {
  const button = (el: HTMLElement, text: string) => [...el.querySelectorAll('button')].find((b) => b.textContent?.trim() === text)!;

  it('maps tri-state flags to select values', () => {
    expect([null, true, false].map(flagToValue)).toEqual(['', 'yes', 'no']);
    expect(['', 'yes', 'no', 'x'].map(valueToFlag)).toEqual([null, true, false, null]);
  });

  it('loads defaults, saves edits and shows the fact preview', async () => {
    const t = mockTauri({
      get_profile: PROFILE,
      get_apply_defaults: { ...EMPTY_DEFAULTS, city: 'Monterrey', needs_sponsorship: false },
      save_apply_defaults: null,
      apply_facts: [{ key: 'city', variants: ['Monterrey'] }],
    });
    TestBed.configureTestingModule({ providers: pageProviders() });
    const f = TestBed.createComponent(ProfilePageComponent);
    f.detectChanges();
    await flush();
    f.detectChanges();
    const el = f.nativeElement as HTMLElement;
    expect(el.querySelector<HTMLInputElement>('#ad-city')!.value).toBe('Monterrey');
    expect(el.querySelector<HTMLSelectElement>('#ad-needs_sponsorship')!.value).toBe('no');
    expect(el.querySelector('details')!.textContent).toContain('Monterrey');

    const relocate = el.querySelector<HTMLSelectElement>('#ad-willing_to_relocate')!;
    relocate.value = 'yes';
    relocate.dispatchEvent(new Event('change'));
    button(el, 'Save defaults').click();
    await flush();
    expect(t.callsTo('save_apply_defaults')[0]['defaults']).toMatchObject({
      city: 'Monterrey',
      needs_sponsorship: false,
      willing_to_relocate: true,
    });
    expect(t.callsTo('apply_facts').length).toBe(2);
  });
});
