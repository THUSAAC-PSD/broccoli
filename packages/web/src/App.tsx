import { PluginRegistryProvider } from '@broccoli/sdk/react';
import './App.css';
import { AppLayout } from './components/AppLayout';
import { ProblemHeader } from './components/ProblemHeader';
import { PluginLoader } from './components/PluginLoader';

// Import plugins
import * as ThemePlugin from './plugins/theme-plugin';
import * as NotificationPlugin from './plugins/notification-plugin';
import * as AnalyticsPlugin from './plugins/analytics-plugin';
import * as AmazingButtonPlugin from './plugins/amazing-button';

// Configure plugins to load
const plugins = [
  ThemePlugin,
  NotificationPlugin,
  AnalyticsPlugin,
  AmazingButtonPlugin,
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
        <ProblemHeader
          id="A"
          title="A + B Problem"
          type="Default"
          io="Standard Input / Output"
          timeLimit="1s"
          memoryLimit="256 MB"
        />
        <div className="p-6">
          <h1 className="text-2xl font-bold">Welcome to Broccoli OJ</h1>
          <p className="text-muted-foreground mt-2">Online Judge Platform</p>
        </div>
      </AppLayout>
    </PluginRegistryProvider>
  );
}

export default App;
