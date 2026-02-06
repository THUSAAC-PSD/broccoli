import type { PluginManifest, ComponentBundle } from '@broccoli/sdk';
import { NotificationCenter } from './components/NotificationCenter';
import { NotificationButton } from './components/NotificationButton';

export const manifest: PluginManifest = {
  name: 'notification-plugin',
  version: '1.0.0',
  description: 'In-app notification system',
  author: 'Broccoli Team',
  slots: [
    {
      name: 'navbar.actions',
      position: 'append',
      component: 'notifications/NotificationButton',
      priority: 50,
    },
    {
      name: 'app.overlay',
      position: 'append',
      component: 'notifications/NotificationCenter',
      priority: 0,
    },
  ],
  enabled: true,
};

export const components: ComponentBundle = {
  'notifications/NotificationButton': NotificationButton,
  'notifications/NotificationCenter': NotificationCenter,
};
