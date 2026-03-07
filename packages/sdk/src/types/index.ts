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
  /**
   * Additional exports are component entries (ElementType).
   * Keyed by the export name referenced in manifest.components values.
   */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  [key: string]: any;
}
