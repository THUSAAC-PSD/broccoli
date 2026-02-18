/**
 * Core type definitions
 */

import type { ElementType, ReactNode } from 'react';

export type SlotPosition =
  | 'append'
  | 'prepend'
  | 'replace'
  | 'before'
  | 'after'
  | 'wrap';

export interface SlotConfig<TContext = unknown> {
  name: string;
  position: SlotPosition;
  component: string;
  target?: string;
  /**
   * Priority for ordering multiple components in the same slot.
   * Higher priority renders first. Default: 0
   */
  priority?: number;
  /**
   * Condition function to determine if component should render.
   * Receives slot context as parameter.
   */
  condition?: (context?: TContext) => boolean;
  /**
   * Props to pass to the component
   */
  props?: Record<string, unknown>;
  /** @internal Populated by getSlots — the plugin that owns this slot */
  _pluginName?: string;
}

export interface RouteConfig {
  /**
   * The URL path for the route (e.g., "dashboard", "contest/:id").
   * Relative to the application root.
   * TODO: Consider nesting routes under a plugin-specific path prefix
   * (e.g., "/plugin-name/route") to avoid conflicts.
   */
  path: string;
  /**
   * The key identifier for the component to render.
   * Must match a key in the plugin's ComponentBundle.
   */
  component: string;
  /**
   * Optional metadata for the route (e.g., page title, breadcrumbs).
   * TODO: Add auth requirements or layout overrides in the future.
   */
  meta?: {
    title?: string;
    [key: string]: unknown;
  };
}

export interface PluginManifest {
  name: string;
  version: string;
  description?: string;
  author?: string;
  slots?: SlotConfig[];
  routes?: RouteConfig[];
  components?: ComponentBundle;
  /**
   * Plugin initialization function called when plugin is registered
   */
  onInit?: () => void | Promise<void>;
  /**
   * Plugin cleanup function called when plugin is unregistered
   */
  onDestroy?: () => void | Promise<void>;
  /**
   * Whether the plugin is enabled by default
   */
  enabled?: boolean;
  /**
   * Translation strings keyed by locale, then by translation key.
   * Example: { 'zh-CN': { 'nav.problems': '题目' } }
   */
  translations?: Record<string, Record<string, string>>;
  // TODO: Add isInitialized flag
}

export interface ComponentBundle {
  [key: string]: ElementType;
}

export interface SlotRenderContext {
  slotName: string;
  children?: ReactNode;
  props?: Record<string, unknown>;
}

export interface PluginModule {
  manifest: PluginManifest;
  components: ComponentBundle;
}
