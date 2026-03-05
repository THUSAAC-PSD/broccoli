import { createContext } from 'react';

import type { ActivePluginManifest, RouteConfig, SlotConfig } from '@/index';
import type { ComponentBundle, PluginModule } from '@/types';

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
  // TODO: reloadPlugin & reloadAllPlugins
  getSlots: (slotName: string) => SlotConfig[];
  errors: Map<string, Error>;
}

export const PluginRegistryContext =
  createContext<PluginRegistryContextValue | null>(null);
