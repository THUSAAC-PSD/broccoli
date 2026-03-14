import { use } from 'react';

import { SidebarStateContext } from '@/sidebar/sidebar-state-context';

export function useSidebarState() {
  const context = use(SidebarStateContext);
  if (!context) {
    throw new Error('useSidebarState must be used within a SidebarProvider');
  }
  return context;
}
