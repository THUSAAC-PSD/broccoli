import type { ActivePluginManifest } from '@broccoli/web-sdk/plugin';

import { ContestAdminActions } from './components/ContestAdminActions';

export { ContestAdminActions };

export const manifest: ActivePluginManifest = {
  id: 'contest-admin',
  name: 'contest-admin',
  entry: '',
  components: {
    'contest/AdminActions': 'ContestAdminActions',
  },
  slots: [
    {
      name: 'contest-overview.content',
      position: 'after',
      component: 'contest/AdminActions',
      priority: 50,
    },
  ],
  routes: [],
  translations: {},
};
