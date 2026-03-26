import { createContext } from 'react';

import type { LoginRequest, User } from '@/auth';

export interface AuthContextValue {
  user: User | null;
  isLoading: boolean;
  /** Authenticated access token (JWT) kept in JS memory. */
  accessToken: string | null;
  login: (data: LoginRequest) => Promise<void>;
  logout: () => Promise<void>;
  refresh: () => Promise<void>;
}

export const AuthContext = createContext<AuthContextValue | null>(null);
