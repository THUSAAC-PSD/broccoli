import { use } from 'react';

import { AuthContext } from '@/auth/auth-context';

export function useAuth() {
  const context = use(AuthContext);
  if (!context) {
    throw new Error('useAuth must be used within an AuthProvider');
  }
  return context;
}

/**
 * True once the initial auth bootstrap has settled: the access token has been
 * refreshed from the session hint, or no session exists. Gate authenticated
 * resource queries on this so they do not fire during the cold-boot window
 * before the token is restored, where they would draw a throwaway 401 and rely
 * on a follow-up refetch. Anonymous users become ready immediately, so a real
 * 401 (and its redirect) still happens for them.
 */
export function useAuthReady() {
  return !useAuth().isLoading;
}
