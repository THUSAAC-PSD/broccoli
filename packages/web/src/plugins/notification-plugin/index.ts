import type { ActivePluginManifest } from '@broccoli/sdk';

import { NotificationButton } from './components/NotificationButton';
import { NotificationCenter } from './components/NotificationCenter';

export { NotificationButton, NotificationCenter };

export const manifest: ActivePluginManifest = {
  id: 'notification-plugin',
  name: 'notification-plugin',
  entry: '',
  components: {
    'notifications/NotificationButton': 'NotificationButton',
    'notifications/NotificationCenter': 'NotificationCenter',
  },
  slots: [
    {
      name: 'app.NotificationButton',
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
  routes: [],
  translations: {
    en: {
      'plugin.notification.button': 'Notifications',
    },
  },
};
