import { useQuery } from '@tanstack/react-query';
import { type ReactNode, useCallback, useMemo, useState } from 'react';

import { useApiClient } from '@/api';
import { I18nContext } from '@/i18n/i18n-context';
import type { TranslationMap } from '@/index';

interface I18nProviderProps {
  children: ReactNode;
  localeKey?: string;
  defaultLocale?: string;
  coreI18n?: Record<string, TranslationMap>;
}

export function I18nProvider({
  children,
  localeKey = 'broccoli-locale',
  defaultLocale = 'en',
  coreI18n = {},
}: I18nProviderProps) {
  const apiClient = useApiClient();

  const [locale, setLocaleState] = useState(() => {
    if (typeof window !== 'undefined') {
      const stored = localStorage.getItem(localeKey);
      // Prevent "undefined" string from being stored
      if (stored && stored !== 'undefined') {
        return stored;
      }
    }
    return defaultLocale;
  });

  const setLocale = useCallback(
    (newLocale: string) => {
      // Prevent invalid locales from being stored
      if (newLocale && newLocale !== 'undefined' && newLocale.length > 0) {
        setLocaleState(newLocale);
        if (typeof window !== 'undefined') {
          localStorage.setItem(localeKey, newLocale);
        }
      }
    },
    [localeKey],
  );

  const { data: availableLocales = [] } = useQuery({
    queryKey: ['i18n', 'locales'],
    queryFn: async () => {
      const { data: pluginLocales = [] } = await apiClient.GET('/i18n/locales');
      const coreLocales = Object.keys(coreI18n);
      return Array.from(new Set([...coreLocales, ...pluginLocales]));
    },
  });

  const { data: pluginTranslations = {}, isLoading } = useQuery({
    queryKey: ['i18n', 'translations', locale],
    queryFn: async () => {
      const { data } = await apiClient.GET('/i18n/translations/{locale}', {
        params: { path: { locale } },
      });
      console.log('Fetched plugin translations for locale', locale);
      return data ?? {};
    },
    // Keep previous translations while loading new ones to prevent UI flickering
    placeholderData: (prev) => prev,
  });

  const t = useCallback(
    (key: string, params?: Record<string, string | number>) => {
      let value = pluginTranslations[key] ?? coreI18n[locale]?.[key] ?? key;
      if (params) {
        for (const [param, replacement] of Object.entries(params)) {
          value = value.replace(`{{${param}}}`, String(replacement));
        }
      }
      return value;
    },
    [locale, pluginTranslations, coreI18n],
  );

  const value = useMemo(
    () => ({
      locale,
      setLocale,
      availableLocales,
      t,
      isLoading,
    }),
    [locale, setLocale, availableLocales, t, isLoading],
  );

  return <I18nContext value={value}>{children}</I18nContext>;
}
