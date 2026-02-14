import { type ReactNode, useCallback, useEffect, useState } from 'react';

import { useApiClient } from '@/api/use-api-client';
import { useTranslation } from '@/i18n';
import { PluginRegistryContext } from '@/plugin/plugin-registry-context';
import type {
  ComponentBundle,
  PluginManifest,
  PluginModule,
  RouteConfig,
  SlotConfig,
} from '@/types';

interface PluginRegistryProviderProps {
  children: ReactNode;
  backendUrl: string;
  // WARN: `pluginModules` is for legacy plugins that need to be loaded from
  // local modules instead of remote URLs. This is a temporary solution until
  // all plugins are migrated to the new system. Will be deprecated.
  pluginModules?: PluginModule[];
}

export function PluginRegistryProvider({
  children,
  backendUrl,
  pluginModules,
}: PluginRegistryProviderProps) {
  const [plugins, setPlugins] = useState<Map<string, PluginManifest>>(
    new Map(),
  );
  const [components, setComponents] = useState<ComponentBundle>({});
  const [routes, setRoutes] = useState<RouteConfig[]>([]);

  const { addTranslations, removeTranslations } = useTranslation();
  const apiClient = useApiClient();

  const loadPluginFromManifest = useCallback(
    async (manifest: PluginManifest, pluginComponents: ComponentBundle) => {
      if (plugins.has(manifest.name)) {
        console.warn(`Plugin '${manifest.name}' is already loaded`);
        return;
      }

      // Call onInit if provided
      if (manifest.onInit) {
        try {
          await manifest.onInit();
        } catch (error) {
          console.error(`Error initializing plugin '${manifest.name}':`, error);
          return;
        }
      }

      setPlugins((prev) => {
        const next = new Map(prev);
        next.set(manifest.name, manifest);
        return next;
      });

      // TODO: Namespace components by plugin name to avoid conflicts
      setComponents((prev) => ({
        ...prev,
        ...pluginComponents,
      }));

      if (manifest.routes) {
        setRoutes((prev) => [...prev, ...(manifest.routes ?? [])]);
      }

      if (manifest.translations) {
        addTranslations(manifest.translations);
      }
    },
    [plugins],
  );

  const loadPluginFromModule = useCallback(
    async (module: PluginModule) => {
      await loadPluginFromManifest(module.manifest, module.components);
    },
    [loadPluginFromManifest],
  );

  const loadPluginFromUrl = useCallback(
    async (url: string) => {
      const pluginModule: PluginModule = await import(/* @vite-ignore */ url);
      await loadPluginFromModule(pluginModule);
    },
    [loadPluginFromModule],
  );

  const loadAllPlugins = useCallback(async () => {
    const { data: pluginList, error } = await apiClient.GET('/plugins/active');

    if (error) {
      throw new Error(`Failed to fetch active plugins: ${error}`);
    }

    await Promise.all(
      pluginList.map(async (pluginInfo) => {
        await loadPluginFromUrl(`${backendUrl}${pluginInfo.entry}`);
      }),
    );

    console.log(`Loaded ${pluginList.length} plugins.`);
  }, [apiClient, backendUrl, loadPluginFromUrl]);

  const unloadPlugin = useCallback(
    async (pluginId: string) => {
      const manifest = plugins.get(pluginId);
      if (!manifest) return;

      // Call onDestroy if provided
      if (manifest.onDestroy) {
        try {
          await manifest.onDestroy();
        } catch (error) {
          console.error(`Error destroying plugin ${pluginId}:`, error);
        }
      }

      setPlugins((prev) => {
        const next = new Map(prev);
        next.delete(pluginId);
        return next;
      });

      // Remove plugin components
      // FIX: components are not exported via plugin manifest
      if (manifest.components) {
        setComponents((prev) => {
          const next = { ...prev };
          Object.keys(manifest.components || {}).forEach((key) => {
            delete next[key];
          });
          return next;
        });
      }

      // Remove plugin routes
      if (manifest.routes) {
        setRoutes((prev) =>
          prev.filter((route) => !manifest.routes?.includes(route)),
        );
      }

      if (manifest.translations) {
        removeTranslations(manifest.translations);
      }
    },
    [plugins],
  );

  const getSlots = useCallback(
    <TContext = unknown,>(
      slotName: string,
      context?: TContext,
    ): SlotConfig<TContext>[] => {
      const slots: SlotConfig<TContext>[] = [];
      plugins.forEach((plugin, pluginName) => {
        // Skip disabled plugins
        if (!plugins.has(pluginName)) return;

        if (plugin.slots) {
          const matchingSlots = plugin.slots.filter((slot) => {
            // Check slot name matches
            if (slot.name !== slotName) return false;

            // Check condition if provided
            if (slot.condition && !slot.condition(context)) return false;

            return true;
          });
          slots.push(...matchingSlots);
        }
      });

      // Sort by priority (higher priority first)
      return slots.sort((a, b) => (b.priority || 0) - (a.priority || 0));
    },
    [plugins],
  );

  useEffect(() => {
    const loadInitialPlugins = async () => {
      await Promise.all(
        (pluginModules ?? []).map(async (module) => {
          await loadPluginFromModule(module);
        }),
      );
    };
    loadInitialPlugins();
  }, [pluginModules, loadPluginFromModule]);

  // Load active plugins from backend on mount
  useEffect(() => {
    loadAllPlugins();
  }, [loadAllPlugins]);

  return (
    <PluginRegistryContext
      value={{
        plugins,
        components,
        routes,
        loadPluginFromManifest,
        loadPluginFromModule,
        loadPluginFromUrl,
        loadAllPlugins,
        unloadPlugin,
        getSlots,
      }}
    >
      {children}
    </PluginRegistryContext>
  );
}
