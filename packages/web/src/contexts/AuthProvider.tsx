import React, { useCallback, useEffect, useMemo, useState } from 'react';

import { api } from '@/lib/api/client';
import { AUTH_TOKEN_KEY } from '@/lib/api/config';

import { AuthContext, type LoginRequest, type User } from './auth-context';

/**
 * AuthProvider component that manages user session state.
 */
export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const logout = useCallback(() => {
    localStorage.removeItem(AUTH_TOKEN_KEY);
    setUser(null);
  }, []);

  const login = useCallback(async (data: LoginRequest) => {
    const { data: resData, error } = await api.POST('/auth/login', {
      body: data,
    });

    if (error) throw new Error(error.message);
    if (!resData) throw new Error('Unexpected login response');

    localStorage.setItem(AUTH_TOKEN_KEY, resData.token);
    setUser({
      id: resData.id,
      username: resData.username,
      role: resData.role,
      permissions: resData.permissions,
    });
  }, []);

  useEffect(() => {
    const initAuth = async () => {
      const token = localStorage.getItem(AUTH_TOKEN_KEY);
      if (!token) {
        setIsLoading(false);
        return;
      }

      const { data: me } = await api.GET('/auth/me');

      if (me) {
        setUser(me);
      } else {
        logout();
      }
      setIsLoading(false);
    };

    initAuth();
  }, [logout]);

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
