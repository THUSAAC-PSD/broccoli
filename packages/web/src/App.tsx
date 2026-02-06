import { PluginRegistryProvider } from '@broccoli/sdk/react';
import './App.css';
import { AppLayout } from './components/AppLayout';
import { PluginLoader } from './components/PluginLoader';
import { ProblemPage } from './pages/ProblemPage';

// Import plugins
import * as ThemePlugin from './plugins/theme-plugin';
import * as NotificationPlugin from './plugins/notification-plugin';
import * as AnalyticsPlugin from './plugins/analytics-plugin';
import * as AmazingButtonPlugin from './plugins/amazing-button';
import * as KeyboardShortcutsPlugin from './plugins/keyboard-shortcuts-plugin';

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
        onError={(name, error) => console.error(`Failed to load ${name}:`, error)}
      />
      <AppLayout>
        <ProblemPage />
      </AppLayout>
    </PluginRegistryProvider>
  );
}

export default App;
