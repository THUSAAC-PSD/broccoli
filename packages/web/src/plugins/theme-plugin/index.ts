import type { ComponentBundle,PluginManifest } from '@broccoli/sdk';

import { ThemeToggle } from './components/ThemeToggle';

export const manifest: PluginManifest = {
  name: 'theme-plugin',
  version: '1.0.0',
  description: 'Provides theme switching functionality',
  author: 'Broccoli Team',
  slots: [
    {
      name: 'sidebar.footer',
      position: 'prepend',
      component: 'theme/ThemeToggle',
      priority: 100, // High priority to appear first
    },
  ],
  enabled: true,
};

export const components: ComponentBundle = {
  'theme/ThemeToggle': ThemeToggle,
};
