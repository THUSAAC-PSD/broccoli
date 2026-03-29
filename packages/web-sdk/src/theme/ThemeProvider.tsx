import { type ReactNode, useEffect, useState } from 'react';

import { ThemeContext } from '@/theme/theme-context';
import type { Theme } from '@/theme/types';

interface ThemeProviderProps {
  children: ReactNode;
  defaultTheme?: Theme;
  storageKey?: string;
}

export function ThemeProvider({
  children,
  defaultTheme = 'light',
  storageKey = 'theme',
  ...props
}: ThemeProviderProps) {
  const [theme, setTheme] = useState<Theme>(() => {
    if (typeof window === 'undefined') return defaultTheme;

    const saved = localStorage.getItem(storageKey) as Theme;
    if (saved) return saved;

    const prefersDark = window.matchMedia(
      '(prefers-color-scheme: dark)',
    ).matches;
    return prefersDark ? 'dark' : 'light';
  });

  useEffect(() => {
    const root = document.documentElement;

    // Remove both classes first
    root.classList.remove('light', 'dark');

    // Add the current theme class
    root.classList.add(theme);

    // Persist to localStorage
    localStorage.setItem(storageKey, theme);
  }, [storageKey, theme]);

  const value = {
    theme,
    setTheme: (newTheme: Theme) => {
      localStorage.setItem(storageKey, newTheme);
      setTheme(newTheme);
    },
  };

  return (
    <ThemeContext {...props} value={value}>
      {children}
    </ThemeContext>
  );
}
