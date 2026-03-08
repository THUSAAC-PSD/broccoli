import type { ActivePluginManifest } from '@broccoli/web-sdk/plugin';

import { ThemeToggle } from './components/ThemeToggle';

export { ThemeToggle };

export const manifest: ActivePluginManifest = {
  id: 'theme-plugin',
  name: 'theme-plugin',
  entry: '',
  components: {
    'theme/ThemeToggle': 'ThemeToggle',
  },
  slots: [
    {
      name: 'sidebar.footer',
      position: 'prepend',
      component: 'theme/ThemeToggle',
      priority: 100, // High priority to appear first
    },
  ],
  routes: [],
  translations: {},
};
