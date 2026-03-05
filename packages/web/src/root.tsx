import './index.css';
import './App.css';

import { ApiClientProvider } from '@broccoli/sdk/api';
import { I18nProvider } from '@broccoli/sdk/i18n';
import { PluginRegistryProvider } from '@broccoli/sdk/plugin';
import { ThemeProvider } from '@broccoli/sdk/theme';
import { QueryClientProvider } from '@tanstack/react-query';
import { useEffect } from 'react';
import { Links, Meta, Outlet, Scripts, ScrollRestoration } from 'react-router';

import { AppLayout } from '@/components/AppLayout';
import { AuthProvider } from '@/contexts/AuthProvider';
import { ContestProvider } from '@/contexts/contest-context';
import { en } from '@/lib/i18n/en';
import { queryClient } from '@/lib/query-client';

import { appConfig } from './config';
// Import plugins
import * as AmazingButtonPlugin from './plugins/amazing-button';
import * as AnalyticsPlugin from './plugins/analytics-plugin';
import * as KeyboardShortcutsPlugin from './plugins/keyboard-shortcuts-plugin';
import * as LocaleSwitcherPlugin from './plugins/locale-switcher-plugin';
import * as NotificationPlugin from './plugins/notification-plugin';
import * as RankingChartsPlugin from './plugins/ranking-charts-plugin';
import * as ThemePlugin from './plugins/theme-plugin';
import * as ZhCNPlugin from './plugins/zh-cn-plugin';

const plugins = [
  ThemePlugin,
  NotificationPlugin,
  AnalyticsPlugin,
  AmazingButtonPlugin,
  KeyboardShortcutsPlugin,
  RankingChartsPlugin,
  ZhCNPlugin,
  LocaleSwitcherPlugin,
];

export function Layout({ children }: { children: React.ReactNode }) {
  useEffect(() => {
    document.documentElement.style.opacity = '';
  }, []);

  return (
    <html
      lang={
        typeof window !== 'undefined'
          ? (localStorage.getItem('broccoli-locale') ?? 'en')
          : 'en'
      }
    >
      <head>
        <meta charSet="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <Meta />
        <Links />
        <script
          dangerouslySetInnerHTML={{
            __html: `(function(){var t=localStorage.getItem('theme');if(!t)t=matchMedia('(prefers-color-scheme:dark)').matches?'dark':'light';document.documentElement.classList.add(t);var l=localStorage.getItem('broccoli-locale');if(l&&l!=='en')document.documentElement.style.opacity='0'})()`,
          }}
        />
      </head>
      <body>
        <QueryClientProvider client={queryClient}>
          <ApiClientProvider
            baseUrl={appConfig.api.baseUrl}
            authTokenKey={appConfig.api.authTokenKey}
          >
            <I18nProvider
              defaultLocale="en"
              defaultTranslations={{ en, ...ZhCNPlugin.manifest.translations }}
            >
              <ThemeProvider defaultTheme="light" storageKey="theme">
                <AuthProvider>
                  <ContestProvider>
                    <PluginRegistryProvider
                      backendUrl={appConfig.plugin.backendUrl}
                      pluginModules={plugins}
                    >
                      <AppLayout>{children}</AppLayout>
                    </PluginRegistryProvider>
                  </ContestProvider>
                </AuthProvider>
              </ThemeProvider>
            </I18nProvider>
          </ApiClientProvider>
        </QueryClientProvider>
        <ScrollRestoration />
        <Scripts />
      </body>
    </html>
  );
}

export default function Root() {
  return <Outlet />;
}
