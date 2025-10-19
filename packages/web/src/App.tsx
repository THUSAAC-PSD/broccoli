import { PluginRegistryProvider } from '@broccoli/sdk/react';
import './App.css';
import { AppLayout } from './components/AppLayout';

function App() {
  return (
    <PluginRegistryProvider>
      <AppLayout>
        <div className="p-6">
          <h1 className="text-2xl font-bold">Welcome to Broccoli OJ</h1>
          <p className="text-muted-foreground mt-2">Online Judge Platform</p>
        </div>
      </AppLayout>
    </PluginRegistryProvider>
  );
}

export default App;
