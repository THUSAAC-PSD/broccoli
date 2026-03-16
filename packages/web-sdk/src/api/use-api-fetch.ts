import { use } from 'react';

import { ApiClientContext } from '@/api/api-client-context';

/**
 * Returns an authenticated fetch function for making raw HTTP requests
 * to API endpoints not covered by the typed client (e.g., plugin-specific
 * endpoints registered at runtime).
 *
 * For typed API calls, prefer `useApiClient()` instead.
 */
export function useApiFetch() {
  const context = use(ApiClientContext);
  if (!context) {
    throw new Error('useApiFetch must be used within an ApiClientProvider');
  }
  return context.apiFetch;
}
