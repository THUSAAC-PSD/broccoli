import './index.css';
import './App.css';

import { I18nProvider } from '@broccoli/sdk/i18n';
import { PluginRegistryProvider } from '@broccoli/sdk/react';
import { QueryClientProvider } from '@tanstack/react-query';
import { Links, Meta, Outlet, Scripts, ScrollRestoration } from 'react-router';

import { AppLayout } from '@/components/AppLayout';
import { PluginLoader } from '@/components/PluginLoader';
import { AuthProvider } from '@/contexts/AuthProvider';
import { en } from '@/lib/i18n/en';
import { queryClient } from '@/lib/query-client';

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
  return (
    <html lang="en">
      <head>
        <meta charSet="UTF-8" />
        <meta name="viewport" content="width=device-width, initial-scale=1.0" />
        <Meta />
        <Links />
      </head>
      <body>
        <QueryClientProvider client={queryClient}>
          <I18nProvider defaultLocale="en" defaultTranslations={{ en }}>
            <PluginRegistryProvider>
              <AuthProvider>
                <PluginLoader plugins={plugins} />
                <AppLayout>{children}</AppLayout>
              </AuthProvider>
            </PluginRegistryProvider>
          </I18nProvider>
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
