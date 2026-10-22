import { TestBed } from '@angular/core/testing';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri } from '../../../testing/tauri-mock';
import type { JobDescription } from '../../core/ipc/types';
import { AUTOSAVE_MS, JdEditorComponent } from './jd-editor.component';

const JOB: JobDescription = { id: 7, title: 'Dev', company: 'Acme', raw_text: 'We need Rust', created_at: '', url: '', status_id: null, company_id: null };

async function render() {
  const t = mockTauri({ update_job_description: null, list_jobs: [] });
  TestBed.configureTestingModule({ providers: pageProviders() });
  const f = TestBed.createComponent(JdEditorComponent);
  f.componentRef.setInput('job', JOB);
  f.detectChanges();
  await flush(); // ngModel writes the value asynchronously
  f.detectChanges();
  const textarea = (f.nativeElement as HTMLElement).querySelector('textarea') as HTMLTextAreaElement;
  return { t, f, textarea, status: () => (f.nativeElement as HTMLElement).querySelector('[role=status]')!.textContent };
}

function type(f: { detectChanges(): void }, textarea: HTMLTextAreaElement, text: string): void {
  textarea.value = text;
  textarea.dispatchEvent(new Event('input'));
  f.detectChanges();
}

describe('JdEditorComponent', () => {
  it('shows the description in an editable text box', async () => {
    const { textarea, status } = await render();
    expect(textarea.value).toBe('We need Rust');
    expect(textarea.readOnly || textarea.disabled).toBe(false);
    expect(status()).toBe('Saved');
  });

  it('saves on blur without waiting for the pause', async () => {
    const { t, f, textarea, status } = await render();
    type(f, textarea, 'We need Rust and Go');
    expect(status()).toBe('Unsaved changes');
    textarea.dispatchEvent(new Event('blur'));
    await flush();
    f.detectChanges();

    expect(t.callsTo('update_job_description')).toEqual([{ id: 7, rawText: 'We need Rust and Go' }]);
    expect(status()).toBe('Saved');
  });

  it('autosaves after a pause in typing', async () => {
    const { t, f, textarea } = await render();
    type(f, textarea, 'Rust');
    await flush();
    expect(t.callsTo('update_job_description')).toEqual([]);

    await new Promise((r) => setTimeout(r, AUTOSAVE_MS + 100));
    expect(t.callsTo('update_job_description')).toEqual([{ id: 7, rawText: 'Rust' }]);
  });

  it('does not save text that is already saved', async () => {
    const { t, textarea } = await render();
    textarea.dispatchEvent(new Event('blur'));
    await flush();
    expect(t.callsTo('update_job_description')).toEqual([]);
  });

  it('keeps the draft and reports the error when a save fails', async () => {
    const t = mockTauri({ update_job_description: () => { throw { kind: 'Io', message: 'disk full' }; }, list_jobs: [] });
    TestBed.configureTestingModule({ providers: pageProviders() });
    const f = TestBed.createComponent(JdEditorComponent);
    f.componentRef.setInput('job', JOB);
    f.detectChanges();
    await flush();
    const textarea = (f.nativeElement as HTMLElement).querySelector('textarea') as HTMLTextAreaElement;
    type(f, textarea, 'Rust and Go');
    textarea.dispatchEvent(new Event('blur'));
    await flush();
    f.detectChanges();

    expect(t.callsTo('update_job_description')).toHaveLength(1);
    expect(textarea.value).toBe('Rust and Go');
    expect((f.nativeElement as HTMLElement).querySelector('[role=status]')!.textContent).toBe('Could not save');
  });
});
