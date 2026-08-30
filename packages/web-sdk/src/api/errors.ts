import type { components } from '@/api/schema';

/**
 * The `{ code, message, details? }` error envelope returned by every API
 * endpoint on failure. Mirrors the server's `ErrorBody` schema component.
 */
export type ApiError = components['schemas']['ErrorBody'];

/**
 * Narrow an unknown thrown value or response body to the server's
 * `ApiError` envelope. Returns `null` when the value doesn't match.
 */
export function parseApiError(value: unknown): ApiError | null {
  if (typeof value !== 'object' || value === null) return null;
  const obj = value as Record<string, unknown>;
  if (typeof obj.code === 'string' && typeof obj.message === 'string') {
    return value as ApiError;
  }
  return null;
}

/**
 * Extract a human-readable error message from an API error envelope or a
 * generic Error. Falls back to the provided fallback string.
 */
export function getErrorMessage(err: unknown, fallback: string): string {
  if (!err) return fallback;

  // API error envelopes and Error instances both carry { message }.
  if (typeof err === 'object' && 'message' in err) {
    const msg = (err as { message?: unknown }).message;
    if (msg && typeof msg === 'string') return msg;
  }

  if (err instanceof Error) return err.message;

  return fallback;
}
