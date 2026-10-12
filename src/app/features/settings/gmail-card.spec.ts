import { type ComponentFixture, TestBed } from '@angular/core/testing';

import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri, type TauriMock } from '../../../testing/tauri-mock';
import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { GmailCardComponent } from './gmail-card.component';

const ACCOUNT = { host: 'imap.gmail.com', port: 993, address: 'diego@gmail.com', has_password: true, folder: '[Gmail]/All Mail' };

async function render(handlers: Record<string, unknown>): Promise<{ t: TauriMock; f: ComponentFixture<GmailCardComponent>; el: HTMLElement }> {
  const t = mockTauri(handlers);
  TestBed.configureTestingModule({ providers: pageProviders() });
  const f = TestBed.createComponent(GmailCardComponent);
  f.detectChanges();
  await flush();
  f.detectChanges();
  return { t, f, el: f.nativeElement as HTMLElement };
}

function button(el: HTMLElement, text: string): HTMLButtonElement {
  return [...el.querySelectorAll('button')].find((b) => b.textContent?.trim() === text)!;
}

function type(el: HTMLElement, label: string, value: string): void {
  const input = el.querySelector(`input[aria-label="${label}"]`) as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event('input'));
}

describe('GmailCardComponent', () => {
  it('shows "Not connected" when no account is stored', async () => {
    const { el } = await render({ get_mail_account: null });

    expect(el.querySelector('[data-testid="gmail-status"]')?.textContent).toContain('Not connected');
  });

  it('shows the connected address when an account exists', async () => {
    const { el } = await render({ get_mail_account: ACCOUNT });

    expect(el.querySelector('[data-testid="gmail-status"]')?.textContent?.trim()).toBe('Connected as diego@gmail.com');
  });

  it('sends a null password on Save when the field is empty', async () => {
    const { t, el } = await render({ get_mail_account: null, save_mail_account: ACCOUNT });
    type(el, 'Gmail address', 'diego@gmail.com');

    button(el, 'Save').click();
    await flush();

    expect(t.callsTo('save_mail_account')).toEqual([
      { host: 'imap.gmail.com', port: 993, address: 'diego@gmail.com', password: null, folder: '[Gmail]/All Mail' },
    ]);
  });

  it('loads the mailboxes after a successful test and saves the chosen one', async () => {
    const { t, el } = await render({
      get_mail_account: null,
      test_mail_account: null,
      list_mail_folders: ['INBOX', '[Gmail]/Todos'],
      save_mail_account: { ...ACCOUNT, folder: 'INBOX' },
    });
    type(el, 'Gmail address', 'diego@gmail.com');

    button(el, 'Test connection').click();
    await flush();
    const select = el.querySelector('select[aria-label="Mailbox to scan"]') as HTMLSelectElement;
    expect([...select.options].map((o) => o.value)).toEqual(['[Gmail]/All Mail', 'INBOX', '[Gmail]/Todos']);

    select.value = 'INBOX';
    select.dispatchEvent(new Event('change'));
    button(el, 'Save').click();
    await flush();

    expect(t.callsTo('save_mail_account')[0]).toMatchObject({ folder: 'INBOX' });
  });

  it('shows the server message in an error toast when the connection test fails', async () => {
    const { el } = await render({
      get_mail_account: ACCOUNT,
      test_mail_account: () => {
        throw { kind: 'MailAuth', message: 'Gmail authentication failed' };
      },
    });
    const toast = TestBed.inject(ZardSonnerService);
    const error = vi.spyOn(toast, 'error');

    button(el, 'Test connection').click();
    await flush();

    expect(error).toHaveBeenCalledWith('Could not connect to Gmail', { description: 'Gmail authentication failed' });
  });
});
