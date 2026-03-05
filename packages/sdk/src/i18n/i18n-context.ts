import { createContext } from 'react';

export interface I18nContextValue {
  locale: string;
  setLocale: (locale: string) => void;
  availableLocales: string[];
  t: (key: string, params?: Record<string, string>) => string;
  isLoading: boolean;
}

export const I18nContext = createContext<I18nContextValue | null>(null);
