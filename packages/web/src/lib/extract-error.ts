/**
 * Extract a human-readable error message from an openapi-fetch error object
 * or a generic Error. Falls back to the provided fallback string.
 */
export function extractErrorMessage(error: unknown, fallback: string): string {
  if (!error) return fallback;

  // openapi-fetch error objects have { code, message }
  if (typeof error === 'object' && 'message' in error) {
    const msg = (error as { message?: string }).message;
    if (msg && typeof msg === 'string') return msg;
  }

  if (error instanceof Error) return error.message;

  return fallback;
}
