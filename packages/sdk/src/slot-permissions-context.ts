import { createContext, use } from 'react';

export interface SlotPermissionsContextValue {
  /** List of permissions the current user has. */
  permissions: string[];
}

export const SlotPermissionsContext =
  createContext<SlotPermissionsContextValue | null>(null);

/**
 * Hook to access the current user's permissions for slot filtering.
 * Returns null if no SlotPermissionsProvider is present (all slots render).
 */
export function useSlotPermissions(): SlotPermissionsContextValue | null {
  return use(SlotPermissionsContext);
}
