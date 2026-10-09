import { TestBed } from '@angular/core/testing';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri, type TauriMock } from '../../../testing/tauri-mock';
import type { ModelSpec } from '../../core/ipc/types';
import { EngineStore } from '../../core/stores/engine.store';
import { ModelsStore, recommend } from '../../core/stores/models.store';
import { FirstRunDialogComponent } from './first-run-dialog.component';
import { FirstRunService } from './first-run.service';

const spec = (id: string, gb: number, size: number): ModelSpec => ({
  id,
  label: id,
  hf_repo: '',
  hf_file: '',
  sha256: '',
  size_bytes: size,
  min_ram_gb: gb,
  kind: 'decision',
});
const SPECS = [spec('clef-flash', 8, 6e9), spec('clef-27b', 20, 17e9), spec('kev-4b', 4, 3e9)];

describe('model recommendation', () => {
  it('prefers the default model when it fits, else the largest that fits', () => {
    expect(recommend(SPECS, 16)?.id).toBe('clef-flash');
    expect(recommend(SPECS, 6)?.id).toBe('kev-4b');
    expect(recommend(SPECS, 2)).toBeNull();
  });
});

describe('FirstRunDialogComponent', () => {
  let t: TauriMock;

  async function render() {
    t = mockTauri({
      get_profile: { name: '', headline: '', email: '', phone: '', location: '', links: [], summary: '' },
      save_profile: null,
      list_models: SPECS.map((s) => ({ spec: s, installed: false })),
      system_ram_gb: 16,
      engine_status: { model_id: null, state: 'none', error: null },
      download_model: () => new Promise(() => undefined),
      list_entries: [],
      list_skills: [],
    });
    TestBed.configureTestingModule({ providers: pageProviders() });
    await TestBed.inject(ModelsStore).load();
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

  it('saves the profile, recommends a model by RAM, and shows download progress', async () => {
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
    expect(body().textContent).toContain('16 GB of RAM');
    const card = body().querySelector('[data-testid=model-clef-flash]') as HTMLElement;
    expect(card.textContent).toContain('Recommended');
    expect((body().querySelector('[data-testid=model-clef-27b] button') as HTMLButtonElement).disabled).toBe(true);

    (card.querySelector('button') as HTMLButtonElement).click();
    await flush();
    await t.emit('model://download', { id: 'clef-flash', done: 3e9, total: 6e9 });
    f.detectChanges();
    expect(card.textContent).toContain('2.8 GB / 5.6 GB');
    expect(card.textContent).toContain('Pause');

    button('Continue while it downloads').click();
    f.detectChanges();
    expect(f.componentInstance.step()).toBe(2);
  });
});
