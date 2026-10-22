import { platform } from '../../core/platform/runtime';

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
  if (isOpenableUrl(url)) void platform.openUrl(url.trim());
}

export function urlHost(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, '');
  } catch {
    return url;
  }
}

/** Any HTTPS posting opens in cvd's apply window; cvd asks before filling an unfamiliar host. */
export function canAutofill(url: string): boolean {
  try {
    return new URL(url.trim()).protocol === 'https:';
  } catch {
    return false;
  }
}
