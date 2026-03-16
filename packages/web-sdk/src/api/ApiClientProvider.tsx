import createClient from 'openapi-fetch';
import { type ReactNode, useCallback, useMemo, useRef } from 'react';

import { ApiClientContext } from '@/api/api-client-context';
import type { paths } from '@/api/schema';
import { AUTH_SESSION_EXPIRED_EVENT } from '@/auth/types';

interface ApiClientProviderProps {
  children: ReactNode;
  baseUrl: string;
}

async function shouldClearAuthToken(response: Response): Promise<boolean> {
  if (response.status !== 401) {
    return false;
  }

  try {
    const body = await response.clone().json();
    // Only trigger a clear if the token is explicitly invalid/expired,
    // not just missing (which allows the AuthProvider to attempt a refresh).
    return body?.code === 'TOKEN_INVALID';
  } catch {
    return false;
  }
}

export function ApiClientProvider({
  children,
  baseUrl,
}: ApiClientProviderProps) {
  const accessTokenRef = useRef<string | null>(null);

  const setAccessToken = useCallback((token: string | null) => {
    accessTokenRef.current = token;
  }, []);

  const clearAuth = useCallback(() => {
    accessTokenRef.current = null;
    if (typeof window !== 'undefined') {
      window.dispatchEvent(new CustomEvent(AUTH_SESSION_EXPIRED_EVENT));
    }
  }, []);

  const apiClient = useMemo(() => {
    const client = createClient<paths>({
      baseUrl,
      credentials: 'include',
    });

    client.use({
      onRequest({ request }) {
        const currentToken = accessTokenRef.current;
        if (currentToken) {
          request.headers.set('Authorization', `Bearer ${currentToken}`);
        }
        return request;
      },
      async onResponse({ response }) {
        if (await shouldClearAuthToken(response)) {
          clearAuth();
        }
        return response;
      },
    });

    return client;
  }, [baseUrl, clearAuth]);

  const apiFetch = useMemo(
    () => async (input: string | URL, init?: RequestInit) => {
      const headers = new Headers(init?.headers);
      const currentToken = accessTokenRef.current;
      if (currentToken && !headers.has('Authorization')) {
        headers.set('Authorization', `Bearer ${currentToken}`);
      }

      const response = await fetch(new URL(input, baseUrl), {
        ...init,
        headers,
      });

      if (await shouldClearAuthToken(response)) {
        clearAuth();
      }

      return response;
    },
    [baseUrl, clearAuth],
  );

  return (
    <ApiClientContext value={{ apiClient, apiFetch, setAccessToken }}>
      {children}
    </ApiClientContext>
  );
}
