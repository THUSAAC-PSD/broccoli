import { use } from 'react';

import { ApiClientContext } from '@/api/api-client-context';

// Hook to access the API client from context
export function useApiClient() {
  const context = use(ApiClientContext);
  if (!context) {
    throw new Error('useApiClient must be used within an ApiClientProvider');
  }
  return context.apiClient;
}
