import { createContext } from 'react';

import type { SidebarState } from '@/sidebar/types';

export interface SidebarStateContextValue {
  sidebarState: SidebarState;
  setSidebarState: (sidebarState: SidebarState) => void;
}

export const SidebarStateContext =
  createContext<SidebarStateContextValue | null>(null);
