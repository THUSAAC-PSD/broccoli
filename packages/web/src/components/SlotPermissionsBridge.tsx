import { SlotPermissionsContext } from '@broccoli/web-sdk/react';
import type { ReactNode } from 'react';

import { useAuth } from '@/features/auth/hooks/use-auth';

/**
 * Bridges the web app's auth context into the SDK's SlotPermissionsContext,
 * so the Slot component can filter entries based on the current user's
 * permissions.
 */
export function SlotPermissionsBridge({ children }: { children: ReactNode }) {
  const { user } = useAuth();
  const permissions = user?.permissions ?? [];

  return (
    <SlotPermissionsContext value={{ permissions }}>
      {children}
    </SlotPermissionsContext>
  );
}
