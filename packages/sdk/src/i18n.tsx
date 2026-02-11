/**
 * @broccoli/sdk/i18n
 * Lightweight i18n system with plugin-extensible translations
 */

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useMemo,
  useState,
} from 'react';

type TranslationMap = Record<string, Record<string, string>>;

interface I18nContextValue {
  locale: string;
  setLocale: (locale: string) => void;
  availableLocales: string[];
  t: (key: string, params?: Record<string, string>) => string;
  addTranslations: (translations: Record<string, Record<string, string>>) => void;
  removeTranslations: (translations: Record<string, Record<string, string>>) => void;
}

const I18nContext = createContext<I18nContextValue | null>(null);

interface I18nProviderProps {
  children: ReactNode;
  defaultLocale?: string;
  defaultTranslations?: TranslationMap;
}

export function I18nProvider({
  children,
  defaultLocale = 'en',
  defaultTranslations = {},
}: I18nProviderProps) {
  const [locale, setLocale] = useState(defaultLocale);
  const [translations, setTranslations] = useState<TranslationMap>(defaultTranslations);

  const addTranslations = useCallback(
    (newTranslations: Record<string, Record<string, string>>) => {
      setTranslations((prev) => {
        const next = { ...prev };
        for (const [loc, keys] of Object.entries(newTranslations)) {
          next[loc] = { ...next[loc], ...keys };
        }
        return next;
      });
    },
    [],
  );

  const removeTranslations = useCallback(
    (toRemove: Record<string, Record<string, string>>) => {
      setTranslations((prev) => {
        const next = { ...prev };
        for (const [loc, keys] of Object.entries(toRemove)) {
          if (next[loc]) {
            const updated = { ...next[loc] };
            for (const key of Object.keys(keys)) {
              delete updated[key];
            }
            next[loc] = updated;
          }
        }
        return next;
      });
    },
    [],
  );

  const availableLocales = useMemo(() => Object.keys(translations), [translations]);

  const t = useCallback(
    (key: string, params?: Record<string, string>): string => {
      let value = translations[locale]?.[key] ?? translations['en']?.[key] ?? key;
      if (params) {
        for (const [param, replacement] of Object.entries(params)) {
          value = value.replace(`{{${param}}}`, replacement);
        }
      }
      return value;
    },
    [locale, translations],
  );

  return (
    <I18nContext.Provider
      value={{ locale, setLocale, availableLocales, t, addTranslations, removeTranslations }}
    >
      {children}
    </I18nContext.Provider>
  );
}

export function useTranslation() {
  const context = useContext(I18nContext);
  if (!context) {
    throw new Error('useTranslation must be used within I18nProvider');
  }
  return context;
}

export function useLocale() {
  const { locale, setLocale, availableLocales } = useTranslation();
  return { locale, setLocale, availableLocales };
}
