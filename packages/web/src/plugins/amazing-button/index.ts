/**
 * Example Plugin: Amazing Button Plugin
 * This demonstrates how plugins work with the slot system
 */

import type { ActivePluginManifest } from '@broccoli/sdk';

import { AmazingButton } from './components/AmazingButton';
import { AmazingPage } from './pages/AmazingPage';

export { AmazingButton, AmazingPage };

export const manifest: ActivePluginManifest = {
  id: 'amazing-button-plugin',
  name: 'amazing-button-plugin',
  entry: '',
  components: {
    'components/AmazingButton': 'AmazingButton',
    'pages/AmazingPage': 'AmazingPage',
  },
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
