import { useAuth } from '@broccoli/web-sdk/auth';
import { SlotPermissionsContext } from '@broccoli/web-sdk/slot';
import type { ReactNode } from 'react';

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
