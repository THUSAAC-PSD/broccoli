import { useCallback, useSyncExternalStore } from 'react';

export type KeybindingMode = 'normal' | 'vim' | 'emacs';

const STORAGE_KEY = 'broccoli-editor-keybinding-mode';
const CHANGE_EVENT = 'broccoli-keybinding-mode-change';

function getSnapshot(): KeybindingMode {
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === 'vim' || stored === 'emacs') return stored;
  return 'normal';
}

function getServerSnapshot(): KeybindingMode {
  return 'normal';
}

function subscribe(callback: () => void): () => void {
  // Cross-tab changes
  const onStorage = (e: StorageEvent) => {
    if (e.key === STORAGE_KEY) callback();
  };
  // Same-tab changes
  const onCustom = () => callback();
  window.addEventListener('storage', onStorage);
  window.addEventListener(CHANGE_EVENT, onCustom);
  return () => {
    window.removeEventListener('storage', onStorage);
    window.removeEventListener(CHANGE_EVENT, onCustom);
  };
}

export function useKeybindingMode(): [
  KeybindingMode,
  (mode: KeybindingMode) => void,
] {
  const mode = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);

  const setMode = useCallback((newMode: KeybindingMode) => {
    if (newMode === 'normal') {
      localStorage.removeItem(STORAGE_KEY);
    } else {
      localStorage.setItem(STORAGE_KEY, newMode);
    }
    window.dispatchEvent(new Event(CHANGE_EVENT));
  }, []);

  return [mode, setMode];
}
