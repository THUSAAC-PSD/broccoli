import type { Client } from 'openapi-fetch';
import { createContext } from 'react';

import type { paths } from '@/api/schema';

export type ApiClient = Client<paths>;

export interface ApiClientContextValue {
  apiClient: ApiClient;
}

export const ApiClientContext = createContext<ApiClientContextValue | null>(
  null,
);
