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
}

export interface PluginManifest {
  name: string;
  version: string;
  description?: string;
  author?: string;
  slots?: SlotConfig[];
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
}

export interface ComponentBundle {
  [key: string]: ElementType;
}

export interface SlotRenderContext {
  slotName: string;
  children?: ReactNode;
  props?: Record<string, unknown>;
}
