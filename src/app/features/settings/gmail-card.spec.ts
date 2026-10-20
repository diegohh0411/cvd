import { type ComponentFixture, TestBed } from '@angular/core/testing';

import { ZardSonnerService } from '@/shared/components/sonner/sonner.service';
import { pageProviders } from '../../../testing/providers';
import { flush, mockTauri, type TauriMock } from '../../../testing/tauri-mock';
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

function button(el: HTMLElement, text: string): HTMLButtonElement | undefined {
  return [...el.querySelectorAll('button')].find((b) => b.textContent?.trim() === text);
}

function type(el: HTMLElement, label: string, value: string): void {
  const input = el.querySelector(`input[aria-label="${label}"]`) as HTMLInputElement;
  input.value = value;
  input.dispatchEvent(new Event('input'));
}

function select(el: HTMLElement, value: string): void {
  const element = el.querySelector('select[aria-label="Mailbox to scan"]') as HTMLSelectElement;
  element.value = value;
  element.dispatchEvent(new Event('change'));
}

describe('GmailCardComponent', () => {
  it('shows "Not connected" when no account is stored', async () => {
    const { el } = await render({ get_mail_account: null });

    expect(el.querySelector('[data-testid="gmail-status"]')?.textContent).toContain('Not connected');
  });

  it('shows the connected address when an account exists', async () => {
    const { el } = await render({ get_mail_account: ACCOUNT, test_mail_account: ['[Gmail]/All Mail'] });

    expect(el.querySelector('[data-testid="gmail-status"]')?.textContent?.trim()).toBe('Connected as diego@gmail.com');
  });

  it('offers no mailbox or Save until the connection test has listed them', async () => {
    const { el } = await render({ get_mail_account: null });

    expect(el.querySelector('select[aria-label="Mailbox to scan"]')).toBeNull();
    expect(button(el, 'Save')).toBeUndefined();
  });

  it('lists the mailboxes after the test, preselects All Mail, and saves the chosen one', async () => {
    const { t, el } = await render({
      get_mail_account: null,
      test_mail_account: ['INBOX', '[Gmail]/All Mail'],
      save_mail_account: { ...ACCOUNT, folder: 'INBOX' },
    });
    type(el, 'Gmail address', 'diego@gmail.com');

    button(el, 'Test connection')!.click();
    await flush();

    const options = [...(el.querySelector('select[aria-label="Mailbox to scan"]') as HTMLSelectElement).options];
    expect(options.map((o) => o.value)).toEqual(['', 'INBOX', '[Gmail]/All Mail']);
    expect((el.querySelector('select[aria-label="Mailbox to scan"]') as HTMLSelectElement).value).toBe('[Gmail]/All Mail');

    select(el, 'INBOX');
    button(el, 'Save')!.click();
    await flush();

    expect(t.callsTo('test_mail_account')[0]).toEqual({ host: 'imap.gmail.com', port: 993, address: 'diego@gmail.com', password: null });
    expect(t.callsTo('save_mail_account')).toEqual([
      { host: 'imap.gmail.com', port: 993, address: 'diego@gmail.com', password: null, folder: 'INBOX' },
    ]);
  });

  it('keeps the saved mailbox when the account still has it', async () => {
    const { el } = await render({
      get_mail_account: { ...ACCOUNT, folder: 'INBOX' },
      test_mail_account: ['INBOX', '[Gmail]/All Mail'],
    });

    expect((el.querySelector('select[aria-label="Mailbox to scan"]') as HTMLSelectElement).value).toBe('INBOX');
  });

  it('shows the server message in an error toast when the connection test fails', async () => {
    const { el } = await render({
      get_mail_account: null,
      test_mail_account: () => {
        throw { kind: 'MailAuth', message: 'Gmail authentication failed' };
      },
    });
    const toast = TestBed.inject(ZardSonnerService);
    const error = vi.spyOn(toast, 'error');
    type(el, 'Gmail address', 'diego@gmail.com');

    button(el, 'Test connection')!.click();
    await flush();

    expect(error).toHaveBeenCalledWith('Could not connect to Gmail', { description: 'Gmail authentication failed' });
    expect(button(el, 'Save')).toBeUndefined();
  });
});
