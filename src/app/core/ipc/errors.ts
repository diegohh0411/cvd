import { AppError, AppErrorKind } from './types';

const KINDS: readonly AppErrorKind[] = [
  'NotFound',
  'ModelMissing',
  'EngineUnavailable',
  'ContextOverflow',
  'Render',
  'Io',
  'ProviderAuth',
  'ProviderCredits',
  'ProviderUnavailable',
];

export function isAppError(e: unknown): e is AppError {
  return (
    typeof e === 'object' &&
    e !== null &&
    KINDS.includes((e as AppError).kind) &&
    typeof (e as AppError).message === 'string'
  );
}

/** Normalizes anything an `invoke()` rejects with into an `AppError`. */
export function toAppError(e: unknown): AppError {
  if (isAppError(e)) return e;
  if (e instanceof Error) return { kind: 'Io', message: e.message };
  return { kind: 'Io', message: typeof e === 'string' ? e : 'Something went wrong.' };
}

export function messageFor(e: unknown): string {
  return toAppError(e).message;
}
