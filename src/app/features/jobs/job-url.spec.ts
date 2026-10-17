import { canAutofill, isOpenableUrl, normalizeUrl, urlHost } from './job-url';

describe('job-url', () => {
  it('adds https to bare hosts and keeps schemes', () => {
    expect(normalizeUrl(' acme.com/jobs/1 ')).toBe('https://acme.com/jobs/1');
    expect(normalizeUrl('http://acme.com')).toBe('http://acme.com');
    expect(normalizeUrl('  ')).toBe('');
  });

  it('only opens http(s) links', () => {
    expect(isOpenableUrl('https://acme.com')).toBe(true);
    expect(isOpenableUrl('file:///etc/passwd')).toBe(false);
    expect(isOpenableUrl('')).toBe(false);
  });

  it('shows the host without www', () => {
    expect(urlHost('https://www.acme.com/careers/1')).toBe('acme.com');
  });

  it('offers autofill only on supported ATS hosts', () => {
    expect(canAutofill('https://job-boards.greenhouse.io/acme/jobs/1')).toBe(true);
    expect(canAutofill('https://jobs.lever.co/acme/1')).toBe(true);
    expect(canAutofill('http://boards.greenhouse.io/acme')).toBe(false);
    expect(canAutofill('https://greenhouse.io.evil.com/x')).toBe(false);
    expect(canAutofill('https://acme.com/careers')).toBe(false);
    expect(canAutofill('')).toBe(false);
  });
});
