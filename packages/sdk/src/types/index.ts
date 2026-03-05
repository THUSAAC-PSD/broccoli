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

export type PluginModule = {
  manifest: ActivePluginManifest;
  /**
   * Plugin initialization function called when plugin is registered
   */
  onInit?: () => void | Promise<void>;
  /**
   * Plugin cleanup function called when plugin is unregistered
   */
  onDestroy?: () => void | Promise<void>;
} & {
  [key in string as key extends 'manifest' ? never : key]: ElementType;
};
