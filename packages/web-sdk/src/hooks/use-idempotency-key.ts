import { useCallback, useRef } from 'react';

function createIdempotencyKey(): string {
  if (typeof globalThis.crypto?.randomUUID === 'function') {
    return globalThis.crypto.randomUUID();
  }

  if (typeof globalThis.crypto?.getRandomValues === 'function') {
    const bytes = new Uint8Array(16);
    globalThis.crypto.getRandomValues(bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0'));

    return [
      hex.slice(0, 4).join(''),
      hex.slice(4, 6).join(''),
      hex.slice(6, 8).join(''),
      hex.slice(8, 10).join(''),
      hex.slice(10, 16).join(''),
    ].join('-');
  }

  return `idemp-${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2)}`;
}

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
      keyRef.current = createIdempotencyKey();
    }
    return keyRef.current;
  }, []);

  /** Reset key after a successful operation (so next operation gets a fresh key). */
  const resetKey = useCallback(() => {
    keyRef.current = null;
  }, []);

  return { getKey, resetKey };
}
