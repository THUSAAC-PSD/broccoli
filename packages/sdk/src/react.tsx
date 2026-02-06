/**
 * @broccoli/sdk/react
 * React-specific exports and hooks
 */

import React, {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useState,
} from 'react';

import type { ComponentBundle, PluginManifest, SlotConfig } from './types';

// Plugin Registry Context
interface PluginRegistryContextValue {
  plugins: Map<string, PluginManifest>;
  components: ComponentBundle;
  enabledPlugins: Set<string>;
  registerPlugin: (
    manifest: PluginManifest,
    components: ComponentBundle,
  ) => Promise<void>;
  unregisterPlugin: (pluginName: string) => Promise<void>;
  enablePlugin: (pluginName: string) => void;
  disablePlugin: (pluginName: string) => void;
  isPluginEnabled: (pluginName: string) => boolean;
  getSlots: <TContext = unknown>(
    slotName: string,
    context?: TContext,
  ) => SlotConfig<TContext>[];
}

const PluginRegistryContext = createContext<PluginRegistryContextValue | null>(
  null,
);

export function PluginRegistryProvider({ children }: { children: ReactNode }) {
  const [plugins, setPlugins] = useState<Map<string, PluginManifest>>(
    new Map(),
  );
  const [components, setComponents] = useState<ComponentBundle>({});
  const [enabledPlugins, setEnabledPlugins] = useState<Set<string>>(new Set());

  const registerPlugin = useCallback(
    async (manifest: PluginManifest, pluginComponents: ComponentBundle) => {
      // Call onInit if provided
      if (manifest.onInit) {
        try {
          await manifest.onInit();
        } catch (error) {
          console.error(`Error initializing plugin ${manifest.name}:`, error);
          return;
        }
      }

      setPlugins((prev) => {
        const next = new Map(prev);
        next.set(manifest.name, manifest);
        return next;
      });

      setComponents((prev) => ({
        ...prev,
        ...pluginComponents,
      }));

      // Enable plugin by default if enabled is not explicitly false
      if (manifest.enabled !== false) {
        setEnabledPlugins((prev) => new Set(prev).add(manifest.name));
      }
    },
    [],
  );

  const unregisterPlugin = useCallback(
    async (pluginName: string) => {
      const plugin = plugins.get(pluginName);
      if (!plugin) return;

      // Call onDestroy if provided
      if (plugin.onDestroy) {
        try {
          await plugin.onDestroy();
        } catch (error) {
          console.error(`Error destroying plugin ${pluginName}:`, error);
        }
      }

      setPlugins((prev) => {
        const next = new Map(prev);
        next.delete(pluginName);
        return next;
      });

      setEnabledPlugins((prev) => {
        const next = new Set(prev);
        next.delete(pluginName);
        return next;
      });

      // Remove plugin components
      if (plugin.components) {
        setComponents((prev) => {
          const next = { ...prev };
          Object.keys(plugin.components || {}).forEach((key) => {
            delete next[key];
          });
          return next;
        });
      }
    },
    [plugins],
  );

  const enablePlugin = useCallback((pluginName: string) => {
    setEnabledPlugins((prev) => new Set(prev).add(pluginName));
  }, []);

  const disablePlugin = useCallback((pluginName: string) => {
    setEnabledPlugins((prev) => {
      const next = new Set(prev);
      next.delete(pluginName);
      return next;
    });
  }, []);

  const isPluginEnabled = useCallback(
    (pluginName: string) => {
      return enabledPlugins.has(pluginName);
    },
    [enabledPlugins],
  );

  const getSlots = useCallback(
    <TContext = unknown,>(
      slotName: string,
      context?: TContext,
    ): SlotConfig<TContext>[] => {
      const slots: SlotConfig<TContext>[] = [];
      plugins.forEach((plugin, pluginName) => {
        // Skip disabled plugins
        if (!enabledPlugins.has(pluginName)) return;

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
    [plugins, enabledPlugins],
  );

  return (
    <PluginRegistryContext.Provider
      value={{
        plugins,
        components,
        enabledPlugins,
        registerPlugin,
        unregisterPlugin,
        enablePlugin,
        disablePlugin,
        isPluginEnabled,
        getSlots,
      }}
    >
      {children}
    </PluginRegistryContext.Provider>
  );
}

export function usePluginRegistry() {
  const context = useContext(PluginRegistryContext);
  if (!context) {
    throw new Error(
      'usePluginRegistry must be used within PluginRegistryProvider',
    );
  }
  return context;
}

// Hook to check if a plugin is enabled
export function usePluginEnabled(pluginName: string) {
  const { isPluginEnabled } = usePluginRegistry();
  return isPluginEnabled(pluginName);
}

// Hook to get all enabled plugins
export function useEnabledPlugins() {
  const { plugins, enabledPlugins } = usePluginRegistry();
  return Array.from(plugins.entries())
    .filter(([name]) => enabledPlugins.has(name))
    .map(([, plugin]) => plugin);
}

// Slot Component
interface SlotProps<TContext = unknown> {
  name: string;
  as?: React.ElementType;
  className?: string;
  children?: ReactNode;
  /**
   * Context object passed to slot condition functions
   */
  context?: TContext;
  /**
   * Additional props to pass to all slot components
   */
  slotProps?: Record<string, unknown>;
}

export function Slot({
  name,
  as = 'div',
  className,
  children,
  context,
  slotProps = {},
}: SlotProps) {
  const { getSlots, components } = usePluginRegistry();
  const slots = getSlots(name, context);
  const Component = as;

  // Render slots based on their position
  const renderSlot = (slot: SlotConfig, index: number) => {
    const SlotComponent = components[slot.component];
    if (!SlotComponent) {
      console.warn(`Component ${slot.component} not found for slot ${name}`);
      return null;
    }

    // Merge slot props with component props
    const componentProps = {
      ...slotProps,
      ...slot.props,
    };

    return (
      <SlotComponent
        key={`${slot.name}-${slot.component}-${index}`}
        {...componentProps}
      />
    );
  };

  // Group slots by position
  const replaceSlots = slots.filter((s) => s.position === 'replace');
  const wrapSlots = slots.filter((s) => s.position === 'wrap');
  const prependSlots = slots.filter((s) => s.position === 'prepend');
  const beforeSlots = slots.filter((s) => s.position === 'before');
  const afterSlots = slots.filter((s) => s.position === 'after');
  const appendSlots = slots.filter((s) => s.position === 'append');

  // Build content based on position types
  let content: ReactNode;

  // If there are replace slots, use them instead of children
  if (replaceSlots.length > 0) {
    content = replaceSlots.map(renderSlot);
  } else {
    // Normal flow: prepend, before, children, after, append
    content = (
      <>
        {beforeSlots.map(renderSlot)}
        {children}
        {afterSlots.map(renderSlot)}
      </>
    );
  }

  // Apply wrap slots (from outermost to innermost)
  wrapSlots.reverse().forEach((slot) => {
    const WrapperComponent = components[slot.component];
    if (WrapperComponent) {
      const wrapperProps = {
        ...slotProps,
        ...slot.props,
        children: content,
      };
      content = <WrapperComponent {...wrapperProps} />;
    }
  });

  return (
    <>
      {prependSlots.map(renderSlot)}
      <Component className={className}>{content}</Component>
      {appendSlots.map(renderSlot)}
    </>
  );
}

// Hook to use plugin components
export function usePluginComponent(componentName: string) {
  const { components } = usePluginRegistry();
  return components[componentName] || null;
}
