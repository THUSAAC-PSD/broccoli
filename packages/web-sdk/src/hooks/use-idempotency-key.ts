import { useCallback, useRef } from 'react';

/**
 * Hook for managing idempotency keys for POST requests.
 *
 * The key is stable across retries of the same logical operation (e.g., double-clicks,
 * network retries), then reset on success so the next operation gets a fresh key.
 *
 * Usage:
 * ```ts
 * const { getKey, resetKey } = useIdempotencyKey();
 *
 * const handleCreate = async () => {
 *   const res = await apiClient.POST('/problems', {
 *     headers: { 'Idempotency-Key': getKey() },
 *     body: { ... },
 *   });
 *   if (res.data) {
 *     resetKey(); // Success, next operation gets a new key
 *   }
 *   // On error, key is preserved -> retry sends same key -> server returns cached response
 * };
 * ```
 */
export function useIdempotencyKey() {
  const keyRef = useRef<string | null>(null);

  /** Get current key, generating one if none exists. */
  const getKey = useCallback(() => {
    if (!keyRef.current) {
      keyRef.current = crypto.randomUUID();
    }
    return keyRef.current;
  }, []);

  /** Reset key after a successful operation (so next operation gets a fresh key). */
  const resetKey = useCallback(() => {
    keyRef.current = null;
  }, []);

  return { getKey, resetKey };
}
