import { createContext, use } from 'react';

import type { components } from '@/lib/api/schema';

export type User = components['schemas']['MeResponse'];
export type LoginRequest = components['schemas']['LoginRequest'];

export interface AuthContextValue {
  user: User | null;
  isLoading: boolean;
  login: (data: LoginRequest) => Promise<void>;
  logout: () => void;
}

export const AuthContext = createContext<AuthContextValue | null>(null);

export function useAuth() {
  const context = use(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
}
