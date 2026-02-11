/**
 * Example Plugin: Amazing Button Plugin
 * This demonstrates how plugins work with the slot system
 */

import type { ComponentBundle, PluginManifest } from '@broccoli/sdk';

import { AmazingButton } from './components/AmazingButton';
import { AmazingPage } from './pages/AmazingPage';

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
  routes: [
    {
      path: '/amazing',
      component: 'pages/AmazingPage',
    },
  ],
  translations: {
    en: {
      'plugin.amazingButton.label': 'Amazing Button',
      'plugin.amazingButton.alert': 'Amazing!',
      'plugin.amazingButton.pageTitle': 'Amazing Page!',
    },
  },
};

export const components: ComponentBundle = {
  'components/AmazingButton': AmazingButton,
  'pages/AmazingPage': AmazingPage,
};
