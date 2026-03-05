/*
  @broccoli/sdk/sidebar
  sidebar management
*/
import { createContext, type ReactNode, use, useEffect, useState } from 'react';

export type SidebarState = 'expanded' | 'collapsed';

export interface StateContextValue {
  sidebarState: SidebarState;
  setSidebarState: (sidebarState: SidebarState) => void;
}

export interface StateProviderProps {
  children: ReactNode;
  defaultState?: SidebarState;
  storageKey?: string;
}

const StateContext = createContext<StateContextValue | null>(null);

export function SidebarProvider({
  children,
  defaultState = 'expanded',
  storageKey = 'sidebar-state',
  ...props
}: StateProviderProps) {
  const [sidebarState, setSidebarState] = useState<SidebarState>(() => {
    if (typeof window !== 'undefined') {
      return (localStorage.getItem(storageKey) as SidebarState) || defaultState;
    }
    return defaultState;
  });

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
    <StateContext {...props} value={value}>
      {children}
    </StateContext>
  );
}

export function useSidebarState() {
  const context = use(StateContext);
  if (!context) {
    throw new Error('useState must be used within a StateProvider');
  }
  return context;
}
