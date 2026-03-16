import { createContext } from 'react';

import type {
  ActivePluginManifest,
  ComponentBundle,
  PluginModule,
  RouteConfig,
  SlotConfig,
} from '@/plugin/types';

// Plugin Registry Context
export interface PluginRegistryContextValue {
  plugins: Map<string, ActivePluginManifest>;
  components: ComponentBundle;
  // TODO: consider appending plugin name to route config,
  // e.g. RouteConfig & { pluginName: string }
  routes: RouteConfig[];
  isLoading: boolean;
  loadPlugin: (
    manifest: ActivePluginManifest,
    module: PluginModule,
  ) => Promise<void>;
  loadAllPlugins: () => Promise<void>;
  unloadPlugin: (pluginId: string) => Promise<void>;
  reloadPlugin: (pluginId: string) => Promise<void>;
  reloadAllPlugins: () => Promise<void>;
  getSlots: (slotName: string) => SlotConfig[];
  errors: Map<string, Error>;
}

export const PluginRegistryContext =
  createContext<PluginRegistryContextValue | null>(null);
