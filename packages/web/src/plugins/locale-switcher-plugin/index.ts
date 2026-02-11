import type { ComponentBundle, PluginManifest } from '@broccoli/sdk';

import { LocaleSwitcher } from './components/LocaleSwitcher';

export const manifest: PluginManifest = {
  name: 'locale-switcher',
  version: '1.0.0',
  description: 'Language switcher for the sidebar',
  author: 'Broccoli Team',
  slots: [
    {
      name: 'sidebar.footer',
      position: 'prepend',
      component: 'locale/LocaleSwitcher',
      priority: 90,
    },
  ],
  enabled: true,
};

export const components: ComponentBundle = {
  'locale/LocaleSwitcher': LocaleSwitcher,
};
