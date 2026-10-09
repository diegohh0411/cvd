import type { Type } from '@angular/core';
import { TestBed } from '@angular/core/testing';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri, type TauriMock } from '../../../testing/tauri-mock';
import type { ModelInfo, ModelSpec } from '../../core/ipc/types';
import { ProvidersPageComponent } from './providers-page.component';

const spec = (id: string, kind: ModelSpec['kind'], size = 2e9): ModelSpec => ({
  id,
  label: id,
  hf_repo: '',
  hf_file: '',
  sha256: '',
  size_bytes: size,
  min_ram_gb: 4,
  kind,
});
const info = (id: string, kind: ModelSpec['kind'], installed: boolean): ModelInfo => ({ spec: spec(id, kind), installed });

async function render<T>(cmp: Type<T>, handlers: Record<string, unknown>) {
  const t: TauriMock = mockTauri(handlers);
  TestBed.configureTestingModule({ providers: pageProviders() });
  const f = TestBed.createComponent(cmp);
  f.detectChanges();
  await flush();
  f.detectChanges();
  return { t, f, el: f.nativeElement as HTMLElement };
}

describe('ProvidersPageComponent', () => {
  const handlers = {
    list_models: [info('kev-4b', 'decision', true), info('chat-1', 'chat', true), info('big', 'decision', false)],
    system_ram_gb: 16,
    engine_status: { model_id: 'kev-4b', state: 'idle', error: null },
    provider_key_status: { has_key: false, last4: null },
  };

  it('shows the local card first, then OpenRouter and Exa', async () => {
    const { el } = await render(ProvidersPageComponent, handlers);
    const titles = [...el.querySelectorAll('[data-testid=provider-title]')].map((e) => e.textContent?.trim());
    expect(titles).toEqual(['Llama.cpp (local)', 'OpenRouter', 'Exa']);
  });

  it('summarizes the local runtime', async () => {
    const { el } = await render(ProvidersPageComponent, handlers);
    const local = el.querySelector('[data-testid=local-provider]')!;
    expect(local.textContent).toContain('16 GB RAM');
    expect(local.textContent).toContain('2 models installed');
    expect(local.textContent).toContain('ready');
    expect(local.querySelector('a')!.getAttribute('href')).toBe('/ai/models');
  });
});
