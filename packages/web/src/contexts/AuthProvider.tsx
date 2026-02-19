import type { LoginRequest, User } from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import React, { useCallback, useEffect, useMemo, useState } from 'react';

import { appConfig } from '@/config';
import { AuthContext } from '@/contexts/auth-context';

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
    },
    [apiClient],
  );

  useEffect(() => {
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
