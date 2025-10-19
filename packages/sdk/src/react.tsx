/**
 * @broccoli/sdk/react
 * React-specific exports and hooks
 */

import React, {
    createContext,
    useContext,
    useState,
    type ReactNode,
} from "react";
import type { PluginManifest, SlotConfig, ComponentBundle } from "./types";

// Plugin Registry Context
interface PluginRegistryContextValue {
    plugins: Map<string, PluginManifest>;
    components: ComponentBundle;
    registerPlugin: (
        manifest: PluginManifest,
        components: ComponentBundle
    ) => void;
    getSlots: (slotName: string) => SlotConfig[];
}

const PluginRegistryContext = createContext<PluginRegistryContextValue | null>(
    null
);

export function PluginRegistryProvider({ children }: { children: ReactNode }) {
    const [plugins, setPlugins] = useState<Map<string, PluginManifest>>(
        new Map()
    );
    const [components, setComponents] = useState<ComponentBundle>({});

    const registerPlugin = (
        manifest: PluginManifest,
        pluginComponents: ComponentBundle
    ) => {
        setPlugins((prev) => {
            const next = new Map(prev);
            next.set(manifest.name, manifest);
            return next;
        });

        setComponents((prev) => ({
            ...prev,
            ...pluginComponents,
        }));
    };

    const getSlots = (slotName: string): SlotConfig[] => {
        const slots: SlotConfig[] = [];
        plugins.forEach((plugin) => {
            if (plugin.slots) {
                const matchingSlots = plugin.slots.filter(
                    (slot) => slot.name === slotName
                );
                slots.push(...matchingSlots);
            }
        });
        return slots;
    };

    return (
        <PluginRegistryContext.Provider
            value={{
                plugins,
                components,
                registerPlugin,
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
            "usePluginRegistry must be used within PluginRegistryProvider"
        );
    }
    return context;
}

// Slot Component
interface SlotProps {
    name: string;
    as?: React.ElementType;
    className?: string;
    children?: ReactNode;
}

export function Slot({ name, as = "div", className, children }: SlotProps) {
    const { getSlots, components } = usePluginRegistry();
    const slots = getSlots(name);
    const Component = as;

    // Render slots based on their position
    const renderSlot = (slot: SlotConfig) => {
        const SlotComponent = components[slot.component];
        if (!SlotComponent) {
            console.warn(
                `Component ${slot.component} not found for slot ${name}`
            );
            return null;
        }
        return <SlotComponent key={`${slot.name}-${slot.component}`} />;
    };

    const appendSlots = slots.filter((s: SlotConfig) => s.position === "after");
    const replaceSlots = slots.filter(
        (s: SlotConfig) => s.position === "replace"
    );
    const beforeSlots = slots.filter(
        (s: SlotConfig) => s.position === "before"
    );

    // If there are replace slots, use them instead of children
    if (replaceSlots.length > 0) {
        return (
            <Component className={className}>
                {replaceSlots.map(renderSlot)}
            </Component>
        );
    }

    return (
        <Component className={className}>
            {beforeSlots.map(renderSlot)}
            {children}
            {appendSlots.map(renderSlot)}
        </Component>
    );
}

// Hook to use plugin components
export function usePluginComponent(componentName: string) {
    const { components } = usePluginRegistry();
    return components[componentName] || null;
}
