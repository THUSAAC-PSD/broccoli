import type { ComponentBundle, PluginManifest } from '@broccoli/sdk';

import { KeyboardShortcutsHandler } from './components/KeyboardShortcutsHandler';

export const manifest: PluginManifest = {
  name: 'keyboard-shortcuts-plugin',
  version: '1.0.0',
  slots: [
    {
      name: 'app.root',
      position: 'wrap',
      component: 'shortcuts/Handler',
      priority: 90,
    },
  ],
  onInit: () => {
    console.log('[Keyboard Shortcuts] Plugin initialized');
    console.log('[Keyboard Shortcuts] Available shortcuts:');
    console.log('  - Ctrl+Enter / Cmd+Enter: Submit code');
    console.log('  - Ctrl+/ / Cmd+/: Toggle fullscreen');
  },
  enabled: true,
};

export const components: ComponentBundle = {
  'shortcuts/Handler': KeyboardShortcutsHandler,
};
