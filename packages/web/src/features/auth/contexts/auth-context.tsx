import type { LoginRequest, User } from '@broccoli/web-sdk';
import { createContext } from 'react';

export interface AuthContextValue {
  user: User | null;
  isLoading: boolean;
  login: (data: LoginRequest) => Promise<void>;
  logout: () => void;
}

export const AuthContext = createContext<AuthContextValue | null>(null);
