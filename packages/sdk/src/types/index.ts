/**
 * Core type definitions
 */

import type { ComponentType } from "react";

export interface SlotConfig {
    name: string;
    position: "append" | "replace" | "before" | "after";
    component: string;
    target?: string;
}

export interface PluginManifest {
    name: string;
    version: string;
    slots?: SlotConfig[];
    components?: Record<string, any>;
}

export interface ComponentBundle {
    [key: string]: ComponentType<any>;
}
