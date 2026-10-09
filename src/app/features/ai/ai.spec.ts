import type { Type } from '@angular/core';
import { type ComponentFixture, TestBed } from '@angular/core/testing';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri, type TauriMock } from '../../../testing/tauri-mock';
import type { ModelInfo, ModelSpec, ProviderModels } from '../../core/ipc/types';
import { ProvidersPageComponent } from './providers-page.component';
import { AiModelsPageComponent } from './models-page.component';
import { RoleCardComponent } from './role-card.component';
import { localOptions, onlineOptions, providerOf, type Role, roleStatus } from './role-options';

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

async function render<T>(cmp: Type<T>, handlers: Record<string, unknown>, init?: (f: ComponentFixture<T>) => void) {
  const t: TauriMock = mockTauri(handlers);
  TestBed.configureTestingModule({ providers: pageProviders() });
  const f = TestBed.createComponent(cmp);
  init?.(f);
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

describe('role options', () => {
  const models = [info('chat-missing', 'chat', false), info('chat-installed', 'chat', true), info('kev-4b', 'decision', true)];
  const remote: ProviderModels[] = [
    { provider: 'openrouter', models: [{ provider: 'openrouter', id: 'deepseek/x', display_name: 'DeepSeek X', context_length: null, price_per_mtok_in: null }], error: null },
  ];
  const kev = info('kev-4b', 'decision', true);

  it('tells providers apart by id', () => {
    expect(providerOf('openrouter:a/b')).toBe('openrouter');
    expect(providerOf('kev-4b')).toBe('local');
    expect(providerOf(null)).toBe('local');
  });

  it('lists local models for the role, installed first', () => {
    expect(localOptions('research', models).map((o) => o.id)).toEqual(['chat-installed', 'chat-missing']);
    expect(localOptions('system-one', models).map((o) => o.id)).toEqual(['kev-4b']);
  });

  it('prefixes online ids with the provider', () => {
    expect(onlineOptions(remote)[0]).toEqual({ id: 'openrouter:deepseek/x', label: 'DeepSeek X', installed: true, detail: '' });
  });

  it('derives the role status', () => {
    expect(roleStatus({ active: null, installed: [], hasOpenRouterKey: false })).toBe('needs-setup');
    expect(roleStatus({ active: 'openrouter:x', installed: [], hasOpenRouterKey: false })).toBe('needs-setup');
    expect(roleStatus({ active: 'openrouter:x', installed: [], hasOpenRouterKey: true })).toBe('ready');
    expect(roleStatus({ active: 'kev-4b', installed: [], hasOpenRouterKey: true })).toBe('needs-setup');
    expect(roleStatus({ active: 'kev-4b', installed: [kev], hasOpenRouterKey: false, hasExaKey: false })).toBe('needs-setup');
    expect(roleStatus({ active: 'kev-4b', installed: [kev], hasOpenRouterKey: false, engineError: true })).toBe('error');
    expect(roleStatus({ active: 'kev-4b', installed: [kev], hasOpenRouterKey: false })).toBe('ready');
  });
});

describe('RoleCardComponent', () => {
  const remote = (error: string | null = null): ProviderModels[] => [
    {
      provider: 'openrouter',
      models: error ? [] : [{ provider: 'openrouter', id: 'deepseek/x', display_name: 'DeepSeek X', context_length: null, price_per_mtok_in: 0 }],
      error,
    },
  ];
  const base = (over: Record<string, unknown> = {}) => ({
    list_models: [info('kev-4b', 'decision', true), info('clef', 'decision', true), info('big', 'decision', false), info('chat-1', 'chat', true)],
    system_ram_gb: 16,
    engine_status: { model_id: 'kev-4b', state: 'idle', error: null },
    provider_key_status: { has_key: true, last4: '1234' },
    get_research_model: 'chat-1',
    list_remote_models: remote(),
    set_active_model: null,
    set_research_model: null,
    get_ui_setting: '1',
    set_ui_setting: null,
    ...over,
  });

  async function card(role: Role, over: Record<string, unknown> = {}) {
    const r = await render(RoleCardComponent, base(over), (f) => f.componentRef.setInput('role', role));
    const q = <E extends Element = HTMLElement>(sel: string) => r.el.querySelector<E>(sel);
    const segment = (label: string) => [...r.el.querySelectorAll('[role=group] button')].find((b) => b.textContent?.includes(label)) as HTMLButtonElement;
    const radio = (id: string) => {
      const b = q<HTMLButtonElement>(`[role=radio][data-id="${id}"]`);
      return b && { el: b, checked: b.getAttribute('aria-checked') === 'true', disabled: b.disabled, click: () => b.click(), closest: (s: string) => b.closest(s) };
    };
    const settle = async () => {
      await flush();
      r.f.detectChanges();
    };
    return { ...r, q, segment, radio, settle };
  }

  it("starts on the active model's provider", async () => {
    const c = await card('system-one', { engine_status: { model_id: 'openrouter:deepseek/x', state: 'idle', error: null } });
    expect(c.segment('OpenRouter').getAttribute('aria-pressed')).toBe('true');
    expect(c.radio('openrouter:deepseek/x')!.checked).toBe(true);
  });

  it('needs setup when key missing for active online model', async () => {
    const c = await card('system-one', {
      engine_status: { model_id: 'openrouter:deepseek/x', state: 'idle', error: null },
      provider_key_status: { has_key: false, last4: null },
    });
    expect(c.q('[data-testid=role-status]')!.textContent).toContain('Needs setup');
    const link = [...c.el.querySelectorAll('a')].find((a) => a.textContent?.includes('Add your OpenRouter key'))!;
    expect(link.getAttribute('href')).toBe('/ai/providers');
    expect(c.t.callsTo('list_remote_models')).toHaveLength(0);
  });

  it('needs setup when active local model is not installed', async () => {
    const c = await card('system-one', { engine_status: { model_id: 'big', state: 'missing', error: null } });
    expect(c.q('[data-testid=role-status]')!.textContent).toContain('Needs setup');
  });

  it('lists not-installed local models with a Download link', async () => {
    const c = await card('system-one');
    expect(c.radio('big')!.disabled).toBe(true);
    const row = c.radio('big')!.closest('li')!;
    expect(row.querySelector('a')!.getAttribute('href')).toBe('/ai/models');
    expect(c.radio('chat-1')).toBeNull();
  });

  it('picking a local model calls set_active_model', async () => {
    const c = await card('system-one');
    c.radio('clef')!.click();
    await c.settle();
    expect(c.t.callsTo('set_active_model')[0]).toEqual({ id: 'clef' });
  });

  it('picking a local research model calls set_research_model', async () => {
    const c = await card('research', { list_models: [info('chat-1', 'chat', true), info('chat-2', 'chat', true)] });
    c.radio('chat-2')!.click();
    await c.settle();
    expect(c.t.callsTo('set_research_model')[0]).toEqual({ id: 'chat-2' });
  });

  it('asks for notice before first OpenRouter pick', async () => {
    const c = await card('system-one', { get_ui_setting: null });
    c.segment('OpenRouter').click();
    await c.settle();
    c.radio('openrouter:deepseek/x')!.click();
    await c.settle();
    expect(c.q('[role=alertdialog]')).not.toBeNull();
    expect(c.t.callsTo('set_active_model')).toHaveLength(0);
    const use = [...c.el.querySelectorAll('[role=alertdialog] button')].find((b) => b.textContent?.includes('Use DeepSeek X')) as HTMLButtonElement;
    use.click();
    await c.settle();
    expect(c.t.callsTo('set_ui_setting')[0]).toMatchObject({ value: '1' });
    expect(c.t.callsTo('set_active_model')[0]).toEqual({ id: 'openrouter:deepseek/x' });
  });

  it('keeps previous selection on failure', async () => {
    const c = await card('system-one', {
      set_active_model: () => {
        throw { kind: 'Engine', message: 'nope' };
      },
    });
    c.radio('clef')!.click();
    await c.settle();
    expect(c.radio('kev-4b')!.checked).toBe(true);
    expect(c.radio('clef')!.checked).toBe(false);
  });

  it('shows inline error when online list fails', async () => {
    const c = await card('system-one', { list_remote_models: remote('boom') });
    c.segment('OpenRouter').click();
    await c.settle();
    expect(c.el.textContent).toContain('boom');
    const retry = [...c.el.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Retry')!;
    expect(retry).toBeTruthy();
    c.segment('Llama.cpp').click();
    await c.settle();
    expect(c.radio('kev-4b')).not.toBeNull();
  });

  it('research card shows Exa requirement', async () => {
    const c = await card('research', {
      provider_key_status: ({ provider }: Record<string, unknown>) => ({ has_key: provider !== 'exa', last4: null }),
    });
    const link = [...c.el.querySelectorAll('a')].find((a) => a.textContent?.includes('Add your Exa key'))!;
    expect(link.getAttribute('href')).toBe('/ai/providers');
    expect(c.q('[data-testid=role-status]')!.textContent).toContain('Needs setup');
  });

  it('system one card shows runtime and restart', async () => {
    const c = await card('system-one');
    expect(c.el.textContent).toContain('llama.cpp on 127.0.0.1');
    const restart = [...c.el.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Restart')!;
    restart.click();
    await c.settle();
    expect(c.t.callsTo('set_active_model')[0]).toEqual({ id: 'kev-4b' });
  });
});

describe('AiModelsPageComponent', () => {
  const handlers = (over: Record<string, unknown> = {}) => ({
    list_models: [info('kev-4b', 'decision', true), info('chat-1', 'chat', false)],
    system_ram_gb: 16,
    engine_status: { model_id: 'kev-4b', state: 'idle', error: null },
    get_research_model: null,
    download_model: null,
    set_research_model: null,
    set_active_model: null,
    ...over,
  });

  it('lists the catalog with RAM info and no Use buttons', async () => {
    const { el } = await render(AiModelsPageComponent, handlers());
    expect(el.textContent).toContain('This computer has 16 GB of RAM.');
    expect(el.querySelector('[data-testid=model-kev-4b]')!.textContent).toContain('Active');
    expect(el.querySelector('[data-testid=model-chat-1]')).not.toBeNull();
    const labels = [...el.querySelectorAll('button')].map((b) => b.textContent?.trim());
    expect(labels).not.toContain('Use this model');
    expect(labels).not.toContain('Use for research');
  });

  it('auto-selects a finished research download when none is set', async () => {
    const { el, t } = await render(AiModelsPageComponent, handlers());
    (el.querySelector('[data-testid=model-chat-1] button') as HTMLButtonElement).click();
    await flush();
    expect(t.callsTo('set_research_model')[0]).toEqual({ id: 'chat-1' });
  });
});
