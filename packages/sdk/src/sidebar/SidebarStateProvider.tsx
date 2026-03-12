import { type ReactNode, useEffect, useState } from 'react';

import { SidebarStateContext } from '@/sidebar/sidebar-state-context';
import type { SidebarState } from '@/sidebar/types';

export interface SidebarStateProviderProps {
  children: ReactNode;
  defaultState?: SidebarState;
  storageKey?: string;
}

export function SidebarStateProvider({
  children,
  defaultState = 'expanded',
  storageKey = 'sidebar-state',
  ...props
}: SidebarStateProviderProps) {
  const [sidebarState, setSidebarState] = useState<SidebarState>(() => {
    // if (typeof window !== 'undefined') {
    //   return (localStorage.getItem(storageKey) as SidebarState) || defaultState;
    // }
    return defaultState;
  });

  useEffect(() => {
    setSidebarState(() => {
      if (typeof window !== 'undefined') {
        return (
          (localStorage.getItem(storageKey) as SidebarState) || defaultState
        );
      }
      return defaultState;
    });
  }, [defaultState, storageKey]);

  useEffect(() => {
    const root = document.documentElement;

    // Remove both classes first
    root.classList.remove('collapsed', 'expanded');

    // Add the current State class
    root.classList.add(sidebarState);

    // Persist to localStorage
    localStorage.setItem('sidebar-state', sidebarState);
  }, [sidebarState]);

  const value = {
    sidebarState,
    setSidebarState: (newState: SidebarState) => {
      localStorage.setItem(storageKey, newState);
      setSidebarState(newState);
    },
  };

  return (
    <SidebarStateContext {...props} value={value}>
      {children}
    </SidebarStateContext>
  );
}
