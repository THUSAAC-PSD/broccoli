/**
 * Plugin Utility Components and Helpers
 * Reusable utilities for building plugins
 */

import type {
  ActivePluginManifest,
  ComponentBundle,
} from '@broccoli/sdk';

type ManifestSlotConfig = ActivePluginManifest['slots'][number];
import type { ReactNode } from 'react';

/**
 * Creates a simple plugin manifest
 */
export function createPlugin(config: {
  name: string;
  slots: ManifestSlotConfig[];
  components: ComponentBundle;
  onInit?: () => void | Promise<void>;
  onDestroy?: () => void | Promise<void>;
}): { manifest: ActivePluginManifest; components: ComponentBundle } {
  return {
    manifest: {
      id: config.name,
      name: config.name,
      entry: '',
      slots: config.slots,
      routes: [],
      components: {},
    },
    components: config.components,
  };
}

/**
 * Wrapper component helper for wrap-position plugins
 */
export function createWrapper(
  WrapperComponent: React.ComponentType<{ children: ReactNode }>,
) {
  return WrapperComponent;
}

/**
 * Create a slot config with common defaults
 */
export function createSlot(config: {
  name: string;
  component: string;
  position?: ManifestSlotConfig['position'];
  priority?: number;
}): ManifestSlotConfig {
  return {
    position: 'append',
    priority: 0,
    ...config,
  };
}

/**
 * Conditional rendering helper for slot components
 */
export function ConditionalRender({
  condition,
  children,
}: {
  condition: boolean;
  children: ReactNode;
}) {
  return condition ? <>{children}</> : null;
}

/**
 * Plugin development mode check
 */
export function isPluginDevMode() {
  return import.meta.env.DEV;
}

/**
 * Plugin storage helper (uses localStorage)
 */
export const pluginStorage = {
  get: (pluginName: string, key: string) => {
    try {
      const data = localStorage.getItem(`plugin:${pluginName}:${key}`);
      return data ? JSON.parse(data) : null;
    } catch {
      return null;
    }
  },

  set: (pluginName: string, key: string, value: unknown) => {
    try {
      localStorage.setItem(
        `plugin:${pluginName}:${key}`,
        JSON.stringify(value),
      );
      return true;
    } catch {
      return false;
    }
  },

  remove: (pluginName: string, key: string) => {
    try {
      localStorage.removeItem(`plugin:${pluginName}:${key}`);
      return true;
    } catch {
      return false;
    }
  },

  clear: (pluginName: string) => {
    try {
      const prefix = `plugin:${pluginName}:`;
      const keysToRemove: string[] = [];

      for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        if (key && key.startsWith(prefix)) {
          keysToRemove.push(key);
        }
      }

      keysToRemove.forEach((key) => localStorage.removeItem(key));
      return true;
    } catch {
      return false;
    }
  },
};

/**
 * Plugin logger helper
 */
export function createPluginLogger(pluginName: string) {
  return {
    log: (...args: unknown[]) => console.log(`[${pluginName}]`, ...args),
    warn: (...args: unknown[]) => console.warn(`[${pluginName}]`, ...args),
    error: (...args: unknown[]) => console.error(`[${pluginName}]`, ...args),
    info: (...args: unknown[]) => console.info(`[${pluginName}]`, ...args),
    debug: (...args: unknown[]) => {
      if (isPluginDevMode()) {
        console.debug(`[${pluginName}]`, ...args);
      }
    },
  };
}
