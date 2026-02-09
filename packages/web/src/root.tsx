import './index.css';
import './App.css';

import { PluginRegistryProvider } from '@broccoli/sdk/react';
import { Links, Meta, Outlet, Scripts, ScrollRestoration } from 'react-router';

import { AppLayout } from '@/components/AppLayout';
import { PluginLoader } from '@/components/PluginLoader';

// Import plugins
import * as AmazingButtonPlugin from './plugins/amazing-button';
import * as AnalyticsPlugin from './plugins/analytics-plugin';
import * as KeyboardShortcutsPlugin from './plugins/keyboard-shortcuts-plugin';
import * as NotificationPlugin from './plugins/notification-plugin';
import * as ThemePlugin from './plugins/theme-plugin';

const plugins = [
  ThemePlugin,
  NotificationPlugin,
  AnalyticsPlugin,
  AmazingButtonPlugin,
  KeyboardShortcutsPlugin,
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
        <PluginRegistryProvider>
          <PluginLoader plugins={plugins} />
          <AppLayout>{children}</AppLayout>
        </PluginRegistryProvider>
        <ScrollRestoration />
        <Scripts />
      </body>
    </html>
  );
}

export default function Root() {
  return <Outlet />;
}
