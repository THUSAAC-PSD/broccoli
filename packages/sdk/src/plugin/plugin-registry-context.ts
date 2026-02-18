import { createContext } from 'react';

import type {
  ComponentBundle,
  PluginManifest,
  PluginModule,
  RouteConfig,
  SlotConfig,
} from '@/types';

// Plugin Registry Context
export interface PluginRegistryContextValue {
  plugins: Map<string, PluginManifest>;
  components: ComponentBundle;
  // TODO: consider appending plugin name to route config,
  // e.g. RouteConfig & { pluginName: string }
  routes: RouteConfig[];
  isLoading: boolean;
  loadPluginFromManifest: (
    manifest: PluginManifest,
    components: ComponentBundle,
  ) => Promise<void>;
  loadPluginFromModule: (module: PluginModule) => Promise<void>;
  loadPluginFromUrl: (url: string) => Promise<void>;
  loadAllPlugins: () => Promise<void>;
  unloadPlugin: (pluginId: string) => Promise<void>;
  // TODO: loadPluginFromId & reloadPlugin & reloadAllPlugins
  getSlots: <TContext = unknown>(
    slotName: string,
    context?: TContext,
  ) => SlotConfig<TContext>[];
}

export const PluginRegistryContext =
  createContext<PluginRegistryContextValue | null>(null);
