import { TestBed } from '@angular/core/testing';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri, type TauriMock } from '../../../testing/tauri-mock';
import { EngineStore } from '../../core/stores/engine.store';
import { FirstRunDialogComponent } from './first-run-dialog.component';
import { FirstRunService } from './first-run.service';

describe('FirstRunDialogComponent', () => {
  let t: TauriMock;

  async function render() {
    t = mockTauri({
      get_profile: { name: '', headline: '', email: '', phone: '', location: '', links: [], summary: '' },
      save_profile: null,
      engine_status: { model_id: null, state: 'none', error: null },
      get_local_connection: { base_url: 'http://127.0.0.1:8080/v1', has_key: false },
      list_connections: [],
      list_entries: [],
      list_skills: [],
    });
    TestBed.configureTestingModule({ providers: pageProviders() });
    await TestBed.inject(EngineStore).load();
    TestBed.inject(FirstRunService).open();
    const f = TestBed.createComponent(FirstRunDialogComponent);
    f.detectChanges();
    await flush();
    f.detectChanges();
    return f;
  }

  const body = () => document.querySelector('[data-testid=first-run-body]') as HTMLElement;
  const button = (label: string) =>
    [...document.querySelectorAll('button')].find((b) => b.textContent?.trim() === label) as HTMLButtonElement;

  it('requires a name before leaving the profile step', async () => {
    const f = await render();
    button('Next').click();
    await flush();
    f.detectChanges();
    expect(body().textContent).toContain('Name is required.');
    expect(f.componentInstance.step()).toBe(0);
    expect(t.callsTo('save_profile')).toHaveLength(0);
  });

  it('saves the profile, offers a server connection, and allows continuing without one', async () => {
    const f = await render();
    const name = document.getElementById('p-name') as HTMLInputElement;
    name.value = 'Ada Lovelace';
    name.dispatchEvent(new Event('input'));
    f.detectChanges();
    button('Next').click();
    await flush();
    f.detectChanges();
    expect(t.callsTo('save_profile')[0]['profile']).toMatchObject({ name: 'Ada Lovelace' });
    expect(f.componentInstance.step()).toBe(1);
    expect(body().querySelector('input[aria-label="Local server base URL"]')).not.toBeNull();
    button('Next').click();
    await flush();
    f.detectChanges();
    expect(f.componentInstance.step()).toBe(2);
  });
});
