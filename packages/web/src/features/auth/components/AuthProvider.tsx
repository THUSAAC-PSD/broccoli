import { useApiClient, useSetApiAccessToken } from '@broccoli/web-sdk/api';
import {
  AUTH_SESSION_EXPIRED_EVENT,
  type LoginRequest,
  type User,
} from '@broccoli/web-sdk/auth';
import React, { useCallback, useEffect, useMemo, useState } from 'react';

import { appConfig } from '@/config';
import { AuthContext } from '@/features/auth/contexts/auth-context';
import { queryClient } from '@/lib/query-client';

// Refresh the access token every 4.5 minutes
const REFRESH_INTERVAL_MS = 4.5 * 60 * 1000;

/**
 * AuthProvider manages the dual-token authentication lifecycle.
 * Access Token (JWT) is stored in memory and synced with ApiClientProvider.
 * Refresh Token is stored in an HttpOnly cookie (handled by the browser).
 * Session Hint is stored in localStorage to persist login intent across reloads.
 */
export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [accessToken, setAccessToken] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const apiClient = useApiClient();
  const setApiAccessToken = useSetApiAccessToken();

  const updateAccessToken = useCallback(
    (token: string | null) => {
      setAccessToken(token);
      setApiAccessToken(token);
    },
    [setApiAccessToken],
  );

  const clearSession = useCallback(() => {
    localStorage.setItem(appConfig.api.sessionStatusKey, 'false');
    updateAccessToken(null);
    setUser(null);
    queryClient.clear();
  }, [updateAccessToken]);

  const logout = useCallback(async () => {
    // Notify backend to revoke the refresh token
    await apiClient.POST('/auth/logout').catch(() => {
      // Ignore network errors on logout
    });
    clearSession();
  }, [apiClient, clearSession]);

  const refresh = useCallback(async () => {
    const { data, error } = await apiClient.POST('/auth/refresh');
    console.log('Refresh response:', { data, error });

    if (error) {
      clearSession();
      return;
    }

    if (data) {
      updateAccessToken(data.token);
      setUser({
        id: data.id,
        username: data.username,
        role: data.role,
        permissions: data.permissions,
      });
      localStorage.setItem(appConfig.api.sessionStatusKey, 'true');
    }
  }, [apiClient, clearSession, updateAccessToken]);

  const login = useCallback(
    async (data: LoginRequest) => {
      const { data: resData, error } = await apiClient.POST('/auth/login', {
        body: data,
      });

      if (error) throw new Error(error.message);
      if (!resData) throw new Error('Unexpected login response');

      updateAccessToken(resData.token);
      setUser({
        id: resData.id,
        username: resData.username,
        role: resData.role,
        permissions: resData.permissions,
      });

      localStorage.setItem(appConfig.api.sessionStatusKey, 'true');
      queryClient.clear();
    },
    [apiClient, updateAccessToken],
  );

  // Sync state with other tabs or infrastructure events
  useEffect(() => {
    const handleTokenCleared = (event: Event) => {
      const key = (event as CustomEvent<{ key?: string }>).detail?.key;
      // If no key provided, clear all. Otherwise, check if it matches our session hint key.
      if (!key || key === appConfig.api.sessionStatusKey) {
        clearSession();
      }
    };

    const handleStorage = (event: StorageEvent) => {
      if (
        event.key === appConfig.api.sessionStatusKey &&
        event.newValue == null
      ) {
        clearSession();
      }
    };

    window.addEventListener(AUTH_SESSION_EXPIRED_EVENT, handleTokenCleared);
    window.addEventListener('storage', handleStorage);

    const initAuth = async () => {
      const sessionHint = localStorage.getItem(appConfig.api.sessionStatusKey);
      if (sessionHint === 'true') {
        await refresh();
      }
      setIsLoading(false);
    };

    initAuth();

    return () => {
      window.removeEventListener(
        AUTH_SESSION_EXPIRED_EVENT,
        handleTokenCleared,
      );
      window.removeEventListener('storage', handleStorage);
    };
  }, [clearSession, refresh]);

  // Automatic background refresh loop
  useEffect(() => {
    if (!accessToken) return;

    const timer = setInterval(() => {
      refresh();
    }, REFRESH_INTERVAL_MS);

    return () => clearInterval(timer);
  }, [accessToken, refresh]);

  const value = useMemo(
    () => ({
      user,
      accessToken,
      isLoading,
      login,
      logout,
      refresh,
    }),
    [user, accessToken, isLoading, login, logout, refresh],
  );

  return <AuthContext value={value}>{children}</AuthContext>;
}
