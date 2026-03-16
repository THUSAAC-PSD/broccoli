import { use } from 'react';

import { ApiClientContext } from '@/api/api-client-context';

export function useSetApiAccessToken() {
  const context = use(ApiClientContext);
  if (!context) {
    throw new Error(
      'useSetApiAccessToken must be used within an ApiClientProvider',
    );
  }
  return context.setAccessToken;
}
