import { createContext } from 'react';

import type { ApiClient } from '@/api/types';

export interface ApiClientContextValue {
  apiClient: ApiClient;
}

export const ApiClientContext = createContext<ApiClientContextValue | null>(
  null,
);
