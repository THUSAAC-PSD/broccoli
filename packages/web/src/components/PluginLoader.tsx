import type { ComponentBundle, PluginManifest } from '@broccoli/sdk';
import { usePluginRegistry } from '@broccoli/sdk/react';
import { useEffect } from 'react';

/**
 * PluginLoader - Automatically loads and registers plugins
 *
 * Usage:
 * ```tsx
 * import { PluginLoader } from '@/components/PluginLoader';
 * import * as ThemePlugin from '@/plugins/theme-plugin';
 * import * as NotificationPlugin from '@/plugins/notification-plugin';
 *
 * const plugins = [ThemePlugin, NotificationPlugin];
 *
 * function App() {
 *   return (
 *     <PluginRegistryProvider>
 *       <PluginLoader plugins={plugins} />
 *       <AppContent />
 *     </PluginRegistryProvider>
 *   );
 * }
 * ```
 */

export interface PluginModule {
  manifest: PluginManifest;
  components: ComponentBundle;
}

export interface PluginLoaderProps {
  /**
   * Array of plugin modules to load
   */
  plugins: PluginModule[];
  /**
   * Callback when all plugins are loaded
   */
  onLoad?: () => void;
  /**
   * Callback when a plugin fails to load
   */
  onError?: (pluginName: string, error: Error) => void;
}

export function PluginLoader({ plugins, onLoad, onError }: PluginLoaderProps) {
  const { registerPlugin } = usePluginRegistry();

  useEffect(() => {
    const loadPlugins = async () => {
      const promises = plugins.map(async (plugin) => {
        try {
          if (!plugin.manifest) {
            throw new Error('Plugin module must export a manifest');
          }

          if (!plugin.components) {
            throw new Error('Plugin module must export components');
          }

          await registerPlugin(plugin.manifest, plugin.components);
        } catch (error) {
          const pluginName = plugin.manifest?.name || 'unknown';
          console.error(`Failed to load plugin ${pluginName}:`, error);
          if (onError) {
            onError(pluginName, error as Error);
          }
        }
      });

      await Promise.all(promises);

      if (onLoad) {
        onLoad();
      }
    };

    loadPlugins();
  }, [plugins, registerPlugin, onLoad, onError]);

  return null;
}

/**
 * useDynamicPluginLoader - Hook for dynamically loading plugins
 *
 * Usage:
 * ```tsx
 * const { loadPlugin, unloadPlugin, loading, error } = useDynamicPluginLoader();
 *
 * const handleLoadPlugin = async () => {
 *   const plugin = await import('@/plugins/my-plugin');
 *   await loadPlugin(plugin);
 * };
 * ```
 */
export function useDynamicPluginLoader() {
  const { registerPlugin, unregisterPlugin } = usePluginRegistry();

  const loadPlugin = async (plugin: PluginModule) => {
    if (!plugin.manifest || !plugin.components) {
      throw new Error('Invalid plugin module');
    }

    await registerPlugin(plugin.manifest, plugin.components);
    return plugin.manifest.name;
  };

  const unloadPlugin = async (pluginName: string) => {
    await unregisterPlugin(pluginName);
  };

  return {
    loadPlugin,
    unloadPlugin,
  };
}
