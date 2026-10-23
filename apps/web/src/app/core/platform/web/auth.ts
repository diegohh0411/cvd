import { createClient, type Session, type SupabaseClient } from '@supabase/supabase-js';

import type { PlatformAuth } from '../platform';

/** Must match `TOKEN_COOKIE` in cvd-server: the event stream reads it, RPC never does. */
const TOKEN_COOKIE = 'cvd_access_token';

interface ServerConfig {
  auth: { supabaseUrl: string; supabaseAnonKey: string } | null;
}

let client: Promise<SupabaseClient | null> | null = null;
/** Whose data this page loaded; `undefined` until the session is first restored. */
let loadedFor: string | null | undefined;

/** `null` when the server runs without accounts (local dev). */
function supabase(): Promise<SupabaseClient | null> {
  client ??= fetch('/api/config')
    .then((res) => res.json() as Promise<ServerConfig>)
    .then(({ auth }) => {
      if (!auth) return null;
      const sb = createClient(auth.supabaseUrl, auth.supabaseAnonKey);
      sb.auth.onAuthStateChange((_event, session) => {
        setCookie(session);
        // Another tab signed out or into a different account: this page's stores, open
        // forms and event stream belong to the old one, so start over.
        if (loadedFor !== undefined && (session?.user.id ?? null) !== loadedFor) location.reload();
      });
      return sb;
    })
    .catch((e: unknown) => {
      client = null; // retry on the next call
      throw e;
    });
  return client;
}

/** `EventSource` can't send headers; the stream reconnects with whatever this holds. */
function setCookie(session: Session | null): void {
  const secure = location.protocol === 'https:' ? '; Secure' : '';
  document.cookie = session
    ? `${TOKEN_COOKIE}=${session.access_token}; Path=/api/events; SameSite=Strict${secure}`
    : `${TOKEN_COOKIE}=; Path=/api/events; Max-Age=0; SameSite=Strict${secure}`;
}

/** Writes the current session to the event-stream cookie; call before connecting. */
export async function syncSession(): Promise<void> {
  const sb = await supabase();
  if (!sb) return;
  const { data } = await sb.auth.getSession();
  setCookie(data.session);
}

/** The current access token, refreshed first if it has expired. */
export async function accessToken(): Promise<string | null> {
  const sb = await supabase();
  if (!sb) return null;
  const { data } = await sb.auth.getSession();
  return data.session?.access_token ?? null;
}

async function required(): Promise<SupabaseClient> {
  const sb = await supabase();
  if (!sb) throw new Error('This server has no sign-in configured.');
  return sb;
}

/** Back to where sign-in started, so OAuth and magic links land on the app. */
const redirectTo = () => `${location.origin}${location.pathname}`;

export const auth: PlatformAuth = {
  async init() {
    const sb = await supabase();
    if (!sb) return null;
    const { data } = await sb.auth.getSession();
    setCookie(data.session);
    loadedFor ??= data.session?.user.id ?? null;
    return { signedIn: data.session !== null, email: data.session?.user.email ?? null };
  },
  async signInWithGoogle() {
    const { error } = await (await required()).auth.signInWithOAuth({ provider: 'google', options: { redirectTo: redirectTo() } });
    if (error) throw error;
  },
  async signInWithEmail(email) {
    // Signs up unknown addresses too; the first request seeds their defaults.
    const { error } = await (await required()).auth.signInWithOtp({ email, options: { emailRedirectTo: redirectTo() } });
    if (error) throw error;
  },
  async signOut() {
    const sb = await supabase();
    await sb?.auth.signOut();
    setCookie(null);
  },
};
