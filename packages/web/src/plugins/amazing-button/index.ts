/**
 * Example Plugin: Amazing Button Plugin
 * This demonstrates how plugins work with the slot system
 */

import type { ComponentBundle,PluginManifest } from '@broccoli/sdk';

import { AmazingButton } from './components/AmazingButton';

export const manifest: PluginManifest = {
  name: 'amazing-button-plugin',
  version: '1.0.0',
  slots: [
    {
      name: 'sidebar.account.menu',
      position: 'append',
      component: 'components/AmazingButton',
    },
  ],
};

export const components: ComponentBundle = {
  'components/AmazingButton': AmazingButton,
};
