import { use } from 'react';

import { PluginRegistryContext } from '@/plugin/plugin-registry-context';

// Hook to access the plugin registry context
export function usePluginRegistry() {
  const context = use(PluginRegistryContext);
  if (!context) {
    throw new Error(
      'usePluginRegistry must be used within PluginRegistryProvider',
    );
  }
  return context;
}

// Hook to check if a plugin is enabled
export function usePluginEnabled(_pluginId: string) {
  // TODO: fetch plugin state from backend
  throw new Error('usePluginEnabled is not implemented yet');
}

// Hook to get all active plugins
export function usePlugins() {
  const { plugins } = usePluginRegistry();
  return Array.from(plugins.entries()).map(([, plugin]) => plugin);
}

// Hook to use plugin components
export function usePluginComponent(componentName: string) {
  const { components } = usePluginRegistry();
  return components[componentName] || null;
}
