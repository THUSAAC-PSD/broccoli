import type { ActivePluginManifest } from '@broccoli/sdk';

import { KeyboardShortcutsHandler } from './components/KeyboardShortcutsHandler';

export { KeyboardShortcutsHandler };

export const manifest: ActivePluginManifest = {
  id: 'keyboard-shortcuts-plugin',
  name: 'keyboard-shortcuts-plugin',
  entry: '',
  components: {
    'shortcuts/Handler': 'KeyboardShortcutsHandler',
  },
  slots: [
    {
      name: 'app.root',
      position: 'wrap',
      component: 'shortcuts/Handler',
      priority: 90,
    },
  ],
  routes: [],
  translations: {},
};

export const onInit = async () => {
  console.log('[Keyboard Shortcuts] Plugin initialized');
  console.log('[Keyboard Shortcuts] Available shortcuts:');
  console.log('  - Ctrl+Enter / Cmd+Enter: Submit code');
  console.log('  - Ctrl+/ / Cmd+/: Toggle fullscreen');
};
