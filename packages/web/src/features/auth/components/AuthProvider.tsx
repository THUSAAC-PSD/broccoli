import { useApiClient } from '@broccoli/web-sdk/api';
import {
  AUTH_TOKEN_CLEARED_EVENT,
  type LoginRequest,
  type User,
} from '@broccoli/web-sdk/auth';
import React, { useCallback, useEffect, useMemo, useState } from 'react';

import { appConfig } from '@/config';
import { AuthContext } from '@/features/auth/contexts/auth-context';
import { queryClient } from '@/lib/query-client';

/**
 * AuthProvider component that manages user session state.
 */
export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const apiClient = useApiClient();

  const logout = useCallback(() => {
    localStorage.removeItem(appConfig.api.authTokenKey);
    setUser(null);
    queryClient.clear();
  }, []);

  const login = useCallback(
    async (data: LoginRequest) => {
      const { data: resData, error } = await apiClient.POST('/auth/login', {
        body: data,
      });

      if (error) throw new Error(error.message);
      if (!resData) throw new Error('Unexpected login response');

      localStorage.setItem(appConfig.api.authTokenKey, resData.token);
      setUser({
        id: resData.id,
        username: resData.username,
        role: resData.role,
        permissions: resData.permissions,
      });
      queryClient.clear();
    },
    [apiClient],
  );

  useEffect(() => {
    const handleTokenCleared = (event: Event) => {
      const key = (event as CustomEvent<{ key?: string }>).detail?.key;
      if (!key || key === appConfig.api.authTokenKey) {
        logout();
      }
    };

    const handleStorage = (event: StorageEvent) => {
      if (event.key === appConfig.api.authTokenKey && event.newValue == null) {
        logout();
      }
    };

    window.addEventListener(AUTH_TOKEN_CLEARED_EVENT, handleTokenCleared);
    window.addEventListener('storage', handleStorage);

    const initAuth = async () => {
      const token = localStorage.getItem(appConfig.api.authTokenKey);
      if (!token) {
        setIsLoading(false);
        return;
      }

      const { data: me } = await apiClient.GET('/auth/me');

      if (me) {
        setUser(me);
      } else {
        logout();
      }
      setIsLoading(false);
    };

    initAuth();

    return () => {
      window.removeEventListener(AUTH_TOKEN_CLEARED_EVENT, handleTokenCleared);
      window.removeEventListener('storage', handleStorage);
    };
  }, [apiClient, logout]);

  const value = useMemo(
    () => ({
      user,
      isLoading,
      login,
      logout,
    }),
    [user, isLoading, login, logout],
  );

  return <AuthContext value={value}>{children}</AuthContext>;
}
