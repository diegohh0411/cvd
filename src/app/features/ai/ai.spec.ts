import type { Type } from '@angular/core';
import { type ComponentFixture, TestBed } from '@angular/core/testing';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri, type TauriMock } from '../../../testing/tauri-mock';
import type { Connection, ModelInfo, ModelSpec, ProviderModels } from '../../core/ipc/types';
import { EngineStore } from '../../core/stores/engine.store';
import { ModelsStore } from '../../core/stores/models.store';
import { AiModelsPageComponent } from './ai-models-page.component';
import { DownloadsPageComponent } from './downloads-page.component';
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

const conn = (id: string, provider: Connection['provider'], label: string): Connection => ({ id, provider, label, last4: '1234' });
const OPENROUTER = conn('openrouter', 'openrouter', 'OpenRouter');
const ALL_KEYS = [OPENROUTER, conn('subq', 'subq', 'SubQ'), conn('typesafe', 'typesafe', 'TypeSafe')];

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

describe('AiModelsPageComponent connections', () => {
  const handlers = {
    list_models: [info('kev-4b', 'decision', true), info('chat-1', 'chat', true), info('big', 'decision', false)],
    system_ram_gb: 16,
    engine_status: { model_id: 'kev-4b', state: 'idle', error: null },
    provider_key_status: { has_key: false, last4: null },
    list_connections: [],
  };

  it('shows the local card first, then OpenRouter, SubQ, TypeSafe and Exa', async () => {
    const { el } = await render(AiModelsPageComponent, { ...handlers, get_research_model: null, list_remote_models: [] });
    const titles = [...el.querySelectorAll('[data-testid=connections] [data-testid=provider-title]')].map((e) => e.textContent?.trim());
    expect(titles).toEqual(['Llama.cpp (local)', 'OpenRouter', 'SubQ', 'TypeSafe', 'Exa']);
  });

  it('summarizes the local runtime', async () => {
    const { el } = await render(AiModelsPageComponent, { ...handlers, get_research_model: null, list_remote_models: [] });
    const local = el.querySelector('[data-testid=local-provider]')!;
    expect(local.textContent).toContain('16 GB RAM');
    expect(local.textContent).toContain('2 models installed');
    expect(local.textContent).toContain('ready');
    expect(local.querySelector('a')!.getAttribute('href')).toBe('/ai/downloads');
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
    expect(providerOf('typesafe:jev-latest')).toBe('typesafe');
    expect(providerOf('openrouter:inception/mercury-decide:free')).toBe('openrouter');
    expect(providerOf('kev-4b')).toBe('local');
    expect(providerOf(null)).toBe('local');
  });

  it('lists local models for the role, installed first', () => {
    expect(localOptions('research', models).map((o) => o.id)).toEqual(['chat-installed', 'chat-missing']);
    expect(localOptions('system-one', models).map((o) => o.id)).toEqual(['kev-4b']);
  });

  it('prefixes online ids with the provider', () => {
    expect(onlineOptions(remote)[0]).toEqual({ id: 'openrouter:deepseek/x', label: 'DeepSeek X', installed: true, detail: '' });
    const subq: ProviderModels[] = [
      { provider: 'subq', models: [{ provider: 'subq', id: 'subq-snap-preview', display_name: 'SubQ Snap', context_length: 393216, price_per_mtok_in: 0.05 }], error: null },
    ];
    expect(onlineOptions(subq)[0]).toEqual({ id: 'subq:subq-snap-preview', label: 'SubQ Snap', installed: true, detail: '$0.05 / M tokens' });
    expect(providerOf('subq:subq-snap-preview')).toBe('subq');
  });

  it('derives status from the active model\'s own key', () => {
    expect(roleStatus({ active: 'subq:subq-snap-preview', installed: [], keys: { openrouter: true, subq: false } })).toBe('needs-setup');
    expect(roleStatus({ active: 'subq:subq-snap-preview', installed: [], keys: { openrouter: false, subq: true } })).toBe('ready');
    expect(roleStatus({ active: 'typesafe:jev-latest', installed: [], keys: { openrouter: true } })).toBe('needs-setup');
    expect(roleStatus({ active: 'typesafe:jev-latest', installed: [], keys: { typesafe: true } })).toBe('ready');
  });

  it('derives the role status', () => {
    expect(roleStatus({ active: null, installed: [], keys: {} })).toBe('needs-setup');
    expect(roleStatus({ active: 'openrouter:x', installed: [], keys: {} })).toBe('needs-setup');
    expect(roleStatus({ active: 'openrouter:x', installed: [], keys: { openrouter: true } })).toBe('ready');
    expect(roleStatus({ active: 'kev-4b', installed: [], keys: { openrouter: true } })).toBe('needs-setup');
    expect(roleStatus({ active: 'kev-4b', installed: [kev], keys: {}, hasExaKey: false })).toBe('needs-setup');
    expect(roleStatus({ active: 'kev-4b', installed: [kev], keys: {}, engineError: true })).toBe('error');
    expect(roleStatus({ active: 'kev-4b', installed: [kev], keys: {} })).toBe('ready');
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
    list_connections: [OPENROUTER],
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
    const settle = async () => {
      await flush();
      r.f.detectChanges();
    };
    /** Expands the picker when it is collapsed. */
    const openPicker = async () => {
      if (!q('[data-testid=picker]')) {
        (q('[data-testid=active-model] button') as HTMLButtonElement).click();
        await settle();
      }
    };
    const radio = (id: string) => {
      const b = q<HTMLButtonElement>(`[role=radio][data-id="${id}"]`);
      return b && { el: b, checked: b.getAttribute('aria-checked') === 'true', disabled: b.disabled, click: () => b.click(), closest: (sel: string) => b.closest(sel) };
    };
    const group = (id: string) => q(`[data-group="${id}"]`);
    return { ...r, q, settle, openPicker, radio, group };
  }

  it('starts collapsed on the active model and where it runs', async () => {
    const c = await card('system-one', { engine_status: { model_id: 'openrouter:deepseek/x', state: 'idle', error: null } });
    expect(c.q('[data-testid=picker]')).toBeNull();
    expect(c.q('[data-testid=active-model]')!.textContent).toContain('DeepSeek X');
    expect(c.q('[data-testid=active-model]')!.textContent).toContain('Online · OpenRouter');
    await c.openPicker();
    expect(c.radio('openrouter:deepseek/x')!.checked).toBe(true);
  });

  it('names the decision slot Fast Model', async () => {
    expect((await card('system-one')).el.textContent).toContain('Fast Model');
  });

  it('names the chat slot Smart Model', async () => {
    expect((await card('research')).el.textContent).toContain('Smart Model');
  });

  it('opens the picker by itself when no model is set', async () => {
    const c = await card('system-one', { engine_status: { model_id: null, state: 'none', error: null } });
    expect(c.q('[data-testid=picker]')).not.toBeNull();
  });

  it('needs setup when key missing for active online model, with an inline key field', async () => {
    const c = await card('system-one', {
      engine_status: { model_id: 'openrouter:deepseek/x', state: 'idle', error: null },
      list_connections: [],
      provider_key_status: { has_key: false, last4: null },
    });
    expect(c.q('[data-testid=role-status]')!.textContent).toContain('Needs setup');
    await c.openPicker();
    expect(c.group('openrouter')!.querySelector('input[aria-label="OpenRouter API key"]')).not.toBeNull();
    expect(c.t.callsTo('list_remote_models')).toHaveLength(0);
  });

  const lists: ProviderModels[] = [
    { provider: 'openrouter', models: [{ provider: 'openrouter', id: 'deepseek/x', display_name: 'DeepSeek X', context_length: null, price_per_mtok_in: 0 }], error: null },
    { provider: 'subq', models: [{ provider: 'subq', id: 'subq-snap-preview', display_name: 'SubQ Snap', context_length: 393216, price_per_mtok_in: 0.05 }], error: null },
    { provider: 'typesafe', models: [{ provider: 'typesafe', id: 'jev-latest', display_name: 'Jev (stable)', context_length: 64000, price_per_mtok_in: 0.042 }], error: null },
  ];

  it('groups models by provider in one picker', async () => {
    const c = await card('system-one', { list_remote_models: lists, list_connections: ALL_KEYS });
    await c.openPicker();
    expect(c.group('subq')!.querySelector('[data-id="subq:subq-snap-preview"]')).not.toBeNull();
    expect(c.group('typesafe')!.querySelector('[data-id="typesafe:jev-latest"]')).not.toBeNull();
    expect(c.group('openrouter')!.querySelector('[data-id="openrouter:deepseek/x"]')).not.toBeNull();
    expect(c.group('local')!.querySelector('[data-id="kev-4b"]')).not.toBeNull();
    expect(c.group('subq')!.querySelector('[data-id="openrouter:deepseek/x"]')).toBeNull();
  });

  it('offers only chat-capable providers for research, never SubQ or TypeSafe', async () => {
    const c = await card('research', { list_remote_models: lists, list_connections: ALL_KEYS });
    await c.openPicker();
    expect(c.group('openrouter')).not.toBeNull();
    expect(c.group('subq')).toBeNull();
    expect(c.group('typesafe')).toBeNull();
    expect(c.t.callsTo('list_remote_models')[0]).toMatchObject({ role: 'chat' });
  });

  it('picking a TypeSafe model calls set_active_model with the typesafe ref', async () => {
    const c = await card('system-one', { list_remote_models: lists, list_connections: ALL_KEYS });
    await c.openPicker();
    c.radio('typesafe:jev-latest')!.click();
    await c.settle();
    expect(c.t.callsTo('set_active_model')[0]).toEqual({ id: 'typesafe:jev-latest' });
  });

  it('shows TypeSafe as an online model in the active summary', async () => {
    const c = await card('system-one', {
      list_remote_models: lists,
      list_connections: ALL_KEYS,
      engine_status: { model_id: 'typesafe:jev-latest', state: 'idle', error: null },
    });
    expect(c.q('[data-testid=active-model]')!.textContent).toContain('Jev (stable)');
    expect(c.q('[data-testid=active-model]')!.textContent).toContain('Online · TypeSafe');
    expect(c.q('[data-testid=role-status]')!.textContent).toContain('Ready');
  });

  it('needs setup when active local model is not installed', async () => {
    const c = await card('system-one', { engine_status: { model_id: 'big', state: 'missing', error: null } });
    expect(c.q('[data-testid=role-status]')!.textContent).toContain('Needs setup');
  });

  it('lists not-installed local models with a Download link', async () => {
    const c = await card('system-one');
    await c.openPicker();
    expect(c.radio('big')!.disabled).toBe(true);
    const row = c.radio('big')!.closest('div.flex')!;
    expect(row.querySelector('a')!.getAttribute('href')).toBe('/ai/downloads');
    expect(c.radio('chat-1')).toBeNull();
  });

  it('filters the picker by search', async () => {
    const c = await card('system-one', { list_remote_models: lists, list_connections: ALL_KEYS });
    await c.openPicker();
    const input = c.q<HTMLInputElement>('input[aria-label="Search models"]')!;
    input.value = 'jev';
    input.dispatchEvent(new Event('input'));
    await c.settle();
    expect(c.radio('typesafe:jev-latest')).not.toBeNull();
    expect(c.radio('openrouter:deepseek/x')).toBeNull();
    expect(c.radio('kev-4b')).toBeNull();
  });

  it('picking a local model calls set_active_model', async () => {
    const c = await card('system-one');
    await c.openPicker();
    c.radio('clef')!.click();
    await c.settle();
    expect(c.t.callsTo('set_active_model')[0]).toEqual({ id: 'clef' });
  });

  it('picking a local research model calls set_research_model', async () => {
    const c = await card('research', { list_models: [info('chat-1', 'chat', true), info('chat-2', 'chat', true)] });
    await c.openPicker();
    c.radio('chat-2')!.click();
    await c.settle();
    expect(c.t.callsTo('set_research_model')[0]).toEqual({ id: 'chat-2' });
  });

  it('asks for notice before first online pick', async () => {
    const c = await card('system-one', { get_ui_setting: null });
    await c.openPicker();
    c.radio('openrouter:deepseek/x')!.click();
    await c.settle();
    expect(c.q('[role=alertdialog]')).not.toBeNull();
    expect(c.q('[role=alertdialog]')!.textContent).toContain('OpenRouter');
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
    await c.openPicker();
    c.radio('clef')!.click();
    await c.settle();
    expect(c.radio('kev-4b')!.checked).toBe(true);
    expect(c.radio('clef')!.checked).toBe(false);
  });

  it('shows an inline error in the provider group when its list fails, leaving local models usable', async () => {
    const c = await card('system-one', { list_remote_models: remote('boom') });
    await c.openPicker();
    expect(c.group('openrouter')!.textContent).toContain('boom');
    const retry = [...c.group('openrouter')!.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Retry');
    expect(retry).toBeTruthy();
    expect(c.radio('kev-4b')).not.toBeNull();
  });

  it('research card asks for the Exa key inline', async () => {
    const c = await card('research', {
      provider_key_status: ({ provider }: Record<string, unknown>) => ({ has_key: provider !== 'exa', last4: null }),
    });
    expect(c.q('input[aria-label="Exa API key"]')).not.toBeNull();
    expect(c.q('[data-testid=role-status]')!.textContent).toContain('Needs setup');
  });

  it('follows the engine when the active model changes elsewhere', async () => {
    let model = 'kev-4b';
    const c = await card('system-one', { engine_status: () => ({ model_id: model, state: 'idle', error: null }) });
    await c.openPicker();
    expect(c.radio('kev-4b')!.checked).toBe(true);
    model = 'clef';
    await c.t.emit('engine://status', {});
    await c.settle();
    expect(c.radio('clef')!.checked).toBe(true);
    const restart = [...c.el.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Restart')!;
    restart.click();
    await c.settle();
    expect(c.t.callsTo('set_active_model')[0]).toEqual({ id: 'clef' });
  });

  it('follows the research model set elsewhere', async () => {
    const c = await card('research', { list_models: [info('chat-1', 'chat', true), info('chat-2', 'chat', true)] });
    await c.openPicker();
    TestBed.inject(ModelsStore).researchModel.set('chat-2');
    await c.settle();
    expect(c.radio('chat-2')!.checked).toBe(true);
  });

  it('ignores a second pick while one is in flight', async () => {
    const c = await card('system-one', { set_active_model: () => new Promise(() => undefined) });
    await c.openPicker();
    c.radio('clef')!.click();
    await c.settle();
    c.radio('kev-4b')!.click();
    await c.settle();
    expect(c.t.callsTo('set_active_model')).toHaveLength(1);
  });

  it('lists each saved key as its own group and routes picks to that key', async () => {
    const work = conn('openrouter~2', 'openrouter', 'Work');
    const both: ProviderModels[] = [
      { provider: 'openrouter', label: 'OpenRouter', models: [{ provider: 'openrouter', id: 'deepseek/x', display_name: 'DeepSeek X', context_length: null, price_per_mtok_in: 0 }], error: null },
      { provider: 'openrouter~2', label: 'Work', models: [{ provider: 'openrouter~2', id: 'deepseek/x', display_name: 'DeepSeek X', context_length: null, price_per_mtok_in: 0 }], error: null },
    ];
    const c = await card('system-one', { list_connections: [OPENROUTER, work], list_remote_models: both });
    await c.openPicker();
    expect(c.group('openrouter')!.querySelector('div')!.textContent).toContain('OpenRouter');
    expect(c.group('openrouter~2')!.querySelector('div')!.textContent).toContain('OpenRouter · Work');
    c.radio('openrouter~2:deepseek/x')!.click();
    await c.settle();
    expect(c.t.callsTo('set_active_model')[0]).toEqual({ id: 'openrouter~2:deepseek/x' });
  });

  it('names the key behind an online model in the summary', async () => {
    const work = conn('openrouter~2', 'openrouter', 'Work');
    const c = await card('system-one', {
      list_connections: [OPENROUTER, work],
      engine_status: { model_id: 'openrouter~2:deepseek/x', state: 'idle', error: null },
    });
    expect(c.q('[data-testid=active-model]')!.textContent).toContain('Online · OpenRouter · Work');
    expect(c.q('[data-testid=role-status]')!.textContent).toContain('Ready');
  });

  it('needs setup when the key behind the active model was removed', async () => {
    const c = await card('system-one', {
      list_connections: [OPENROUTER],
      engine_status: { model_id: 'openrouter~2:deepseek/x', state: 'idle', error: null },
    });
    expect(c.q('[data-testid=role-status]')!.textContent).toContain('Needs setup');
  });

  it('fast model card shows runtime and restart', async () => {
    const c = await card('system-one');
    expect(c.el.textContent).toContain('llama.cpp on 127.0.0.1');
    const restart = [...c.el.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Restart')!;
    restart.click();
    await c.settle();
    expect(c.t.callsTo('set_active_model')[0]).toEqual({ id: 'kev-4b' });
  });

  it('reports a remote runtime for online models', async () => {
    const c = await card('system-one', {
      list_remote_models: lists,
      list_connections: ALL_KEYS,
      engine_status: { model_id: 'typesafe:jev-latest', state: 'idle', error: null },
    });
    expect(c.el.textContent).toContain('Online model');
    expect(c.el.textContent).not.toContain('llama.cpp on 127.0.0.1');
  });
});

describe('DownloadsPageComponent', () => {
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
    const { el } = await render(DownloadsPageComponent, handlers());
    expect(el.textContent).toContain('This computer has 16 GB of RAM.');
    expect(el.querySelector('[data-testid=model-kev-4b]')!.textContent).toContain('Active');
    expect(el.querySelector('[data-testid=model-chat-1]')).not.toBeNull();
    const labels = [...el.querySelectorAll('button')].map((b) => b.textContent?.trim());
    expect(labels).not.toContain('Use this model');
    expect(labels).not.toContain('Use for research');
  });

  it('auto-selects a finished research download when none is set', async () => {
    const { el, t } = await render(DownloadsPageComponent, handlers());
    (el.querySelector('[data-testid=model-chat-1] button') as HTMLButtonElement).click();
    await flush();
    expect(t.callsTo('set_research_model')[0]).toEqual({ id: 'chat-1' });
  });
});

describe('provider connections', () => {
  const handlers = (over: Record<string, unknown> = {}) => ({
    list_models: [info('kev-4b', 'decision', true)],
    system_ram_gb: 16,
    engine_status: { model_id: 'kev-4b', state: 'idle', error: null },
    provider_key_status: { has_key: false, last4: null },
    get_research_model: null,
    list_remote_models: [],
    list_connections: [OPENROUTER, conn('openrouter~2', 'openrouter', 'Work')],
    ...over,
  });
  const section = (el: HTMLElement, provider: string) =>
    [...el.querySelectorAll('[data-testid=connections] app-provider-connections')].find((e) => e.querySelector('[data-testid=provider-title]')!.textContent?.trim() === provider)!;

  it('lists every saved key of a provider with its name', async () => {
    const { el } = await render(AiModelsPageComponent, handlers());
    const rows = [...section(el, 'OpenRouter').querySelectorAll('[data-connection]')];
    expect(rows.map((r) => r.getAttribute('data-connection'))).toEqual(['openrouter', 'openrouter~2']);
    expect(rows[1].textContent).toContain('Work');
    expect(rows[1].textContent).toContain('…1234');
  });

  it('adds a named key through add_connection', async () => {
    const { el, t, f } = await render(AiModelsPageComponent, handlers({ list_connections: [], add_connection: conn('openrouter', 'openrouter', 'Personal') }));
    const box = section(el, 'OpenRouter');
    const name = box.querySelector<HTMLInputElement>('input[aria-label="OpenRouter key name"]')!;
    const key = box.querySelector<HTMLInputElement>('input[aria-label="OpenRouter API key"]')!;
    name.value = 'Personal';
    name.dispatchEvent(new Event('input'));
    key.value = 'sk-or-1234';
    key.dispatchEvent(new Event('input'));
    f.detectChanges();
    [...box.querySelectorAll('button')].find((b) => b.textContent?.includes('Save'))!.click();
    await flush();
    expect(t.callsTo('add_connection')[0]).toEqual({ provider: 'openrouter', label: 'Personal', key: 'sk-or-1234' });
  });

  it('removes one key by its connection id', async () => {
    const { el, t, f } = await render(AiModelsPageComponent, handlers({ delete_provider_key: null }));
    const row = section(el, 'OpenRouter').querySelector('[data-connection="openrouter~2"]')!;
    [...row.querySelectorAll('button')].find((b) => b.textContent?.includes('Remove'))!.click();
    await flush();
    f.detectChanges();
    expect(t.callsTo('delete_provider_key')[0]).toEqual({ provider: 'openrouter~2' });
  });
});

describe('EngineStore remote detection', () => {
  it.each(['openrouter:x/y', 'openrouter~2:x/y:free', 'subq:subq-snap-preview', 'typesafe:jev-latest'])('treats %s as online', (id) => {
    mockTauri({});
    TestBed.configureTestingModule({ providers: pageProviders() });
    const engine = TestBed.inject(EngineStore);
    engine.status.set({ model_id: id, state: 'idle', error: null });
    expect(engine.remote()).toBe(true);
    expect(engine.modelLabel()).toContain('(online)');
  });

  it('treats a bare id as local', () => {
    mockTauri({});
    TestBed.configureTestingModule({ providers: pageProviders() });
    const engine = TestBed.inject(EngineStore);
    engine.status.set({ model_id: 'clef-flash', state: 'idle', error: null });
    expect(engine.remote()).toBe(false);
  });
});
