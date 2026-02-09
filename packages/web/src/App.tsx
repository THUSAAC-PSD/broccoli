import './App.css';

import { PluginRegistryProvider } from '@broccoli/sdk/react';
import { createRouter, RouterProvider } from '@tanstack/react-router';

import { PluginLoader } from './components/PluginLoader';
// Import plugins
import * as AmazingButtonPlugin from './plugins/amazing-button';
import * as AnalyticsPlugin from './plugins/analytics-plugin';
import * as KeyboardShortcutsPlugin from './plugins/keyboard-shortcuts-plugin';
import * as NotificationPlugin from './plugins/notification-plugin';
import * as ThemePlugin from './plugins/theme-plugin';
import { routeTree } from './routeTree.gen';

const router = createRouter({
  routeTree,
  defaultPreload: 'intent',
  scrollRestoration: true,
});

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}

// Configure plugins to load
const plugins = [
  ThemePlugin,
  NotificationPlugin,
  AnalyticsPlugin,
  AmazingButtonPlugin,
  KeyboardShortcutsPlugin,
];

function App() {
  return (
    <PluginRegistryProvider>
      <PluginLoader
        plugins={plugins}
        onLoad={() => console.log('All plugins loaded successfully')}
        onError={(name, error) =>
          console.error(`Failed to load ${name}:`, error)
        }
      />
      <RouterProvider router={router} />
    </PluginRegistryProvider>
  );
}

export default App;
