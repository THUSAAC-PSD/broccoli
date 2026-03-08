/**
 * Core type definitions
 */

import type { ElementType, ReactNode } from 'react';

import type { ActivePluginManifest } from '@/index';

export interface ComponentBundle {
  [key: string]: ElementType;
}

export interface SlotRenderContext {
  slotName: string;
  children?: ReactNode;
  props?: Record<string, unknown>;
}

export interface PluginModule {
  manifest: ActivePluginManifest;
  /**
   * Plugin initialization function called when plugin is registered
   */
  onInit?: () => void | Promise<void>;
  /**
   * Plugin cleanup function called when plugin is unregistered
   */
  onDestroy?: () => void | Promise<void>;
  /** Dynamic component exports keyed by name */
  [key: string]: unknown;
}

/**
 * A lazy plugin loader: a function that returns a promise resolving to a
 * PluginModule. Used for code-splitting plugins so they are only fetched
 * when the application mounts rather than being included in the main bundle.
 */
export type LazyPluginLoader = () => Promise<PluginModule>;
