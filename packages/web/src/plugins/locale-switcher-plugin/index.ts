import type { ActivePluginManifest } from '@broccoli/web-sdk/plugin';

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
      position: 'append',
      component: 'locale/LocaleSwitcher',
      priority: 90,
    },
  ],
  routes: [],
  translations: {},
};
