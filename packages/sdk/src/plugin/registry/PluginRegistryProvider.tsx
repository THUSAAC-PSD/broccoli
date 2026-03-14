import { useQueryClient } from '@tanstack/react-query';
import {
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';

import { useApiClient } from '@/api/use-api-client';
import { PluginRegistryContext } from '@/plugin/registry/plugin-registry-context';
import type {
  ActivePluginManifest,
  ComponentBundle,
  LazyPluginLoader,
  PluginModule,
  RouteConfig,
  SlotConfig,
} from '@/plugin/types';

interface PluginRegistryProviderProps {
  children: ReactNode;
  backendUrl: string;
  /**
   * Eagerly loaded plugin modules (included in the main bundle).
   * @deprecated Use `lazyPlugins` for code-splitting.
   */
  pluginModules?: PluginModule[];
  /**
   * Lazy plugin loaders. Each loader is a function returning a dynamic
   * `import()` so Vite can code-split them into separate chunks.
   */
  lazyPlugins?: LazyPluginLoader[];
}

export function PluginRegistryProvider({
  children,
  backendUrl,
  pluginModules,
  lazyPlugins,
}: PluginRegistryProviderProps) {
  const [plugins, setPlugins] = useState<Map<string, ActivePluginManifest>>(
    () => new Map(),
  );

  const activeManifests = useRef<Map<string, ActivePluginManifest>>(new Map());
  const activeModules = useRef<Map<string, PluginModule>>(new Map());

  const [components, setComponents] = useState<ComponentBundle>({});
  const [routes, setRoutes] = useState<RouteConfig[]>([]);

  const componentOwnersRef = useRef<Map<string, string>>(new Map());

  // TODO: loadingLock
  const [localLoaded, setLocalLoaded] = useState(false);
  const [remoteLoaded, setRemoteLoaded] = useState(false);
  const isLoading = !localLoaded || !remoteLoaded;

  const [errors, setErrors] = useState<Map<string, Error>>(() => new Map());

  const apiClient = useApiClient();
  const queryClient = useQueryClient();

  const resolvePluginEntryUrl = useCallback(
    (entry: string, bustCache = false) => {
      const url = new URL(entry, backendUrl);
      if (bustCache) {
        url.searchParams.set('_ts', `${Date.now()}`);
      }
      return url.toString();
    },
    [backendUrl],
  );

  const refreshI18n = useCallback(async () => {
    await Promise.all([
      queryClient.refetchQueries({
        queryKey: ['i18n', 'locales'],
        type: 'active',
      }),
      queryClient.refetchQueries({
        queryKey: ['i18n', 'translations'],
        type: 'active',
      }),
    ]);
  }, [queryClient]);

  const unloadPlugin = useCallback(async (pluginId: string) => {
    const manifest = activeManifests.current.get(pluginId);
    const module = activeModules.current.get(pluginId);

    if (!manifest || !module) {
      return;
    }

    try {
      // Call onDestroy if provided
      await module.onDestroy?.();

      // Remove plugin components
      if (manifest.components) {
        setComponents((prev) => {
          const next = { ...prev };
          Object.keys(manifest.components || {}).forEach((key) => {
            delete next[key];
            componentOwnersRef.current.delete(key);
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

      activeManifests.current.delete(pluginId);
      activeModules.current.delete(pluginId);
      setPlugins(new Map(activeManifests.current));
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));
      console.error(
        `Error unloading plugin '${manifest.name}' with id '${manifest.id}':`,
        err,
      );
      setErrors((prev) => new Map(prev).set(pluginId, err));
    }
  }, []);

  const loadPlugin = useCallback(
    async (manifest: ActivePluginManifest, module: PluginModule) => {
      if (activeManifests.current.has(manifest.id)) {
        console.warn(
          `Plugin '${manifest.name}' with id '${manifest.id}' is already loaded. Skipping.`,
        );
        return;
      }

      try {
        // Call onInit if provided
        await module.onInit?.();

        activeManifests.current.set(manifest.id, manifest);
        activeModules.current.set(manifest.id, module);

        if (manifest.components) {
          const resolvedComponents: ComponentBundle = {};
          for (const [key, name] of Object.entries(manifest.components)) {
            if (module[name]) {
              resolvedComponents[key] = module[name] as React.ElementType;
            } else {
              console.warn(
                `Component '${name}' specified in plugin '${manifest.name}' not found in module. Skipping component '${key}'.`,
              );
            }
          }

          // Check for component namespace collisions before merging
          for (const key of Object.keys(resolvedComponents)) {
            const existingOwner = componentOwnersRef.current.get(key);
            if (existingOwner && existingOwner !== manifest.name) {
              console.warn(
                `Component key '${key}' from plugin '${manifest.name}' ` +
                  `overwrites existing component from plugin '${existingOwner}'.`,
              );
            }
            componentOwnersRef.current.set(key, manifest.name);
          }

          setComponents((prev) => ({
            ...prev,
            ...resolvedComponents,
          }));
        }

        if (manifest.routes) {
          setRoutes((prev) => [...prev, ...(manifest.routes ?? [])]);
        }

        setPlugins(new Map(activeManifests.current));
        console.log(
          `Plugin '${manifest.name}' with id '${manifest.id}' loaded successfully.`,
        );
      } catch (error) {
        const err = error instanceof Error ? error : new Error(String(error));
        console.error(
          `Error loading plugin '${manifest.name}' with id '${manifest.id}':`,
          err,
        );
        setErrors((prev) => new Map(prev).set(manifest.id, err));
        await unloadPlugin(manifest.id);
      }
    },
    [unloadPlugin],
  );

  const loadRemotePlugins = useCallback(
    async (bustCache = false) => {
      const { data: pluginList, error } =
        await apiClient.GET('/plugins/active');

      if (error) {
        console.warn(`Failed to fetch active plugins:`, error);
        return;
      }

      const results = await Promise.allSettled(
        pluginList.map(async (pluginInfo) => {
          const pluginModule: PluginModule = pluginInfo.entry
            ? await import(
                /* @vite-ignore */ resolvePluginEntryUrl(
                  pluginInfo.entry,
                  bustCache,
                )
              )
            : {}; // For translation-only plugins without an entry point
          await loadPlugin(pluginInfo, pluginModule);
        }),
      );

      const failed = results.filter(
        (r): r is PromiseRejectedResult => r.status === 'rejected',
      );
      await refreshI18n();
      if (failed.length > 0) {
        console.warn(
          `${failed.length}/${pluginList.length} plugins failed to load.`,
        );
        for (const r of failed) {
          console.error('Plugin load error:', r.reason);
        }
      }
    },
    [apiClient, loadPlugin, refreshI18n, resolvePluginEntryUrl],
  );

  const loadAllPlugins = useCallback(
    async () => loadRemotePlugins(false),
    [loadRemotePlugins],
  );

  const reloadPlugin = useCallback(
    async (pluginId: string) => {
      await unloadPlugin(pluginId);

      setErrors((prev) => {
        const next = new Map(prev);
        next.delete(pluginId);
        return next;
      });

      const { data: pluginList, error } =
        await apiClient.GET('/plugins/active');
      if (error) {
        console.warn(`Failed to fetch active plugins for reload:`, error);
        return;
      }

      const pluginInfo = pluginList.find((p) => p.id === pluginId);
      if (!pluginInfo) {
        console.warn(
          `Plugin '${pluginId}' not found in active plugins after reload`,
        );
        return;
      }

      // Dynamic import with new URL (cache buster ensures fresh module)
      try {
        const pluginModule: PluginModule = pluginInfo.entry
          ? await import(
              /* @vite-ignore */ resolvePluginEntryUrl(pluginInfo.entry, true)
            )
          : {};
        await loadPlugin(pluginInfo, pluginModule);
        await refreshI18n();
      } catch (err) {
        console.error(`Failed to reload plugin '${pluginId}':`, err);
        setErrors((prev) =>
          new Map(prev).set(
            pluginId,
            err instanceof Error ? err : new Error(String(err)),
          ),
        );
      }
    },
    [apiClient, loadPlugin, refreshI18n, resolvePluginEntryUrl, unloadPlugin],
  );

  const reloadAllPlugins = useCallback(async () => {
    const remotePluginIds: string[] = [];
    activeManifests.current.forEach((manifest, id) => {
      if (manifest.entry) {
        remotePluginIds.push(id);
      }
    });

    for (const id of remotePluginIds) {
      await unloadPlugin(id);
    }

    setErrors((prev) => {
      const next = new Map(prev);
      for (const id of remotePluginIds) {
        next.delete(id);
      }
      return next;
    });

    await loadRemotePlugins(true);
  }, [loadRemotePlugins, unloadPlugin]);

  const getSlots = useCallback(
    (slotName: string): SlotConfig[] => {
      const slots: SlotConfig[] = [];
      plugins.forEach((plugin, pluginName) => {
        if (plugin.slots) {
          const matchingSlots = plugin.slots
            .filter((slot) => slot.name === slotName)
            .map((slot) => ({ ...slot, _pluginName: pluginName }));
          slots.push(...matchingSlots);
        }
      });

      // Sort by priority (higher priority first)
      return slots.sort((a, b) => (b.priority || 0) - (a.priority || 0));
    },
    [plugins],
  );

  // Load local plugin modules on mount (eager + lazy)
  useEffect(() => {
    const loadInitialPlugins = async () => {
      // Load eagerly-imported plugins
      const eagerLoads = (pluginModules ?? []).map(async (module) => {
        await loadPlugin(module.manifest, module);
      });

      // Load lazy plugins (code-split via dynamic import)
      const lazyLoads = (lazyPlugins ?? []).map(async (loader) => {
        try {
          const module = await loader();
          await loadPlugin(module.manifest, module);
        } catch (error) {
          console.error('Failed to load lazy plugin:', error);
        }
      });

      await Promise.all([...eagerLoads, ...lazyLoads]);
      setLocalLoaded(true);
    };
    loadInitialPlugins();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Load active plugins from backend on mount
  useEffect(() => {
    const load = async () => {
      await loadAllPlugins();
      setRemoteLoaded(true);
      await refreshI18n();
    };
    load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <PluginRegistryContext
      value={{
        plugins,
        components,
        routes,
        isLoading,
        loadPlugin,
        loadAllPlugins,
        unloadPlugin,
        reloadPlugin,
        reloadAllPlugins,
        getSlots,
        errors,
      }}
    >
      {children}
    </PluginRegistryContext>
  );
}
