import { createContext } from 'react';

import type { ApiClient } from '@/api/types';

export type ApiFetch = (
  input: string | URL,
  init?: RequestInit,
) => Promise<Response>;

export interface ApiClientContextValue {
  apiClient: ApiClient;
  apiFetch: ApiFetch;
  /** Sets the in-memory access token used for requests. */
  setAccessToken: (token: string | null) => void;
}

export const ApiClientContext = createContext<ApiClientContextValue | null>(
  null,
);
