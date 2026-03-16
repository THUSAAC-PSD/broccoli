import { use } from 'react';

import { ApiClientContext } from '@/api/api-client-context';

export function useApiFetch() {
  const context = use(ApiClientContext);
  if (!context) {
    throw new Error('useApiFetch must be used within an ApiClientProvider');
  }
  return context.apiFetch;
}
