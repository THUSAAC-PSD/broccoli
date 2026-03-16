import type { DockviewApi, SerializedDockview } from 'dockview-react';
import { useCallback, useEffect, useRef } from 'react';

export const DOCK_STORAGE_KEY = 'broccoli-dock-problem';
const DEBOUNCE_MS = 300;

export function useDockLayout() {
  const apiRef = useRef<DockviewApi | null>(null);
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const disposeRef = useRef<{ dispose: () => void } | null>(null);

  const saveLayout = useCallback(() => {
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    saveTimerRef.current = setTimeout(() => {
      const api = apiRef.current;
      if (!api) return;
      try {
        const json = api.toJSON();
        localStorage.setItem(DOCK_STORAGE_KEY, JSON.stringify(json));
      } catch {
        // localStorage full or serialization error — ignore
      }
    }, DEBOUNCE_MS);
  }, []);

  const restoreLayout = useCallback((): SerializedDockview | null => {
    try {
      const raw = localStorage.getItem(DOCK_STORAGE_KEY);
      if (!raw) return null;
      return JSON.parse(raw) as SerializedDockview;
    } catch {
      return null;
    }
  }, []);

  const resetLayout = useCallback(() => {
    localStorage.removeItem(DOCK_STORAGE_KEY);
  }, []);

  const setApi = useCallback(
    (api: DockviewApi) => {
      disposeRef.current?.dispose();
      apiRef.current = api;
      disposeRef.current = api.onDidLayoutChange(() => saveLayout());
    },
    [saveLayout],
  );

  // Cleanup on unmount: dispose listener and cancel pending saves
  useEffect(() => {
    return () => {
      disposeRef.current?.dispose();
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    };
  }, []);

  return { setApi, restoreLayout, resetLayout, apiRef };
}
