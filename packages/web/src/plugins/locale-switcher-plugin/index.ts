import type { ActivePluginManifest } from '@broccoli/sdk';

import { LocaleSwitcher } from './components/LocaleSwitcher';

export { LocaleSwitcher };

export const manifest: ActivePluginManifest = {
  id: 'locale-switcher',
  name: 'locale-switcher',
  entry: '',
  components: {
    'locale/LocaleSwitcher': 'LocaleSwitcher',
  },
  slots: [
    {
      name: 'sidebar.footer',
      position: 'prepend',
      component: 'locale/LocaleSwitcher',
      priority: 90,
    },
  ],
  routes: [],
  translations: {},
};
