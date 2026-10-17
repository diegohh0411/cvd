import { openUrl } from '@tauri-apps/plugin-opener';

/** Adds https:// to bare hosts ("acme.com/jobs/1"); '' stays ''. */
export function normalizeUrl(raw: string): string {
  const url = raw.trim();
  if (!url || /^[a-z][a-z0-9+.-]*:/i.test(url)) return url;
  return `https://${url}`;
}

/** Only http(s) links are opened in the system browser. */
export function isOpenableUrl(url: string): boolean {
  return /^https?:\/\/\S+$/i.test(url.trim());
}

export function openJobUrl(url: string): void {
  if (isOpenableUrl(url)) void openUrl(url.trim());
}

export function urlHost(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, '');
  } catch {
    return url;
  }
}

/** Hosts auto-apply works on; mirrors `autofill::window::SUPPORTED_HOSTS`. */
const AUTOFILL_HOSTS = ['greenhouse.io', 'jobs.lever.co', 'jobs.ashbyhq.com'];

export function canAutofill(url: string): boolean {
  try {
    const u = new URL(url.trim());
    return u.protocol === 'https:' && AUTOFILL_HOSTS.some((h) => u.hostname === h || u.hostname.endsWith(`.${h}`));
  } catch {
    return false;
  }
}
