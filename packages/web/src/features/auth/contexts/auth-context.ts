import type { LoginRequest, User } from '@broccoli/web-sdk/auth';
import { createContext } from 'react';

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
