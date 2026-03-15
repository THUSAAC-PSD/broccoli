import createClient from 'openapi-fetch';
import { type ReactNode, useMemo } from 'react';

import { ApiClientContext } from '@/api/api-client-context';
import type { paths } from '@/api/schema';
import { AUTH_TOKEN_CLEARED_EVENT } from '@/auth/types';

interface ApiClientProviderProps {
  children: ReactNode;
  baseUrl: string;
  authTokenKey: string;
}

async function shouldClearAuthToken(response: Response): Promise<boolean> {
  if (response.status !== 401) {
    return false;
  }

  try {
    const body = await response.clone().json();
    return body?.code === 'TOKEN_INVALID';
  } catch {
    return false;
  }
}

export function ApiClientProvider({
  children,
  baseUrl,
  authTokenKey,
}: ApiClientProviderProps) {
  const clearAuthToken = useMemo(
    () => () => {
      localStorage.removeItem(authTokenKey);
      if (typeof window !== 'undefined') {
        window.dispatchEvent(
          new CustomEvent(AUTH_TOKEN_CLEARED_EVENT, {
            detail: { key: authTokenKey },
          }),
        );
      }
    },
    [authTokenKey],
  );

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
      async onResponse({ response }) {
        if (await shouldClearAuthToken(response)) {
          clearAuthToken();
        }
        return response;
      },
    });

    return client;
  }, [baseUrl, authTokenKey, clearAuthToken]);

  const apiFetch = useMemo(
    () => async (input: string | URL, init?: RequestInit) => {
      const headers = new Headers(init?.headers);
      const token = localStorage.getItem(authTokenKey);
      if (token && !headers.has('Authorization')) {
        headers.set('Authorization', `Bearer ${token}`);
      }

      const response = await fetch(new URL(input, baseUrl), {
        ...init,
        headers,
      });

      if (await shouldClearAuthToken(response)) {
        clearAuthToken();
      }

      return response;
    },
    [authTokenKey, baseUrl, clearAuthToken],
  );

  return (
    <ApiClientContext value={{ apiClient, apiFetch }}>
      {children}
    </ApiClientContext>
  );
}
