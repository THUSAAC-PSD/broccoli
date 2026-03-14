import createClient from 'openapi-fetch';
import { type ReactNode, useMemo } from 'react';

import { ApiClientContext } from '@/api/api-client-context';
import type { paths } from '@/api/schema';

interface ApiClientProviderProps {
  children: ReactNode;
  baseUrl: string;
  authTokenKey: string;
}

export function ApiClientProvider({
  children,
  baseUrl,
  authTokenKey,
}: ApiClientProviderProps) {
  const apiClient = useMemo(() => {
    const client = createClient<paths>({
      baseUrl,
      // TODO: headers
    });

    client.use({
      onRequest({ request }) {
        const token = localStorage.getItem(authTokenKey);
        if (token) {
          request.headers.set('Authorization', `Bearer ${token}`);
        }
        return request;
      },
      onResponse({ response }) {
        if (response.status === 401) {
          localStorage.removeItem(authTokenKey);
        }
        return response;
      },
    });

    return client;
  }, [baseUrl, authTokenKey]);

  return <ApiClientContext value={{ apiClient }}>{children}</ApiClientContext>;
}
