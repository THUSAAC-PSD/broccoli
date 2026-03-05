import type { ActivePluginManifest } from '@broccoli/sdk';

import {
  ContestCountdown,
  ContestCountdownMini,
} from './components/ContestCountdown';

export { ContestCountdown, ContestCountdownMini };

export const manifest: ActivePluginManifest = {
  id: 'contest-countdown',
  name: 'contest-countdown',
  entry: '',
  components: {
    'contest/Countdown': 'ContestCountdown',
    'contest/CountdownMini': 'ContestCountdownMini',
  },
  slots: [
    {
      name: 'contest-detail.header',
      position: 'prepend',
      component: 'contest/Countdown',
      priority: 100,
    },
    {
      name: 'problem-detail.header',
      position: 'append',
      component: 'contest/CountdownMini',
      priority: 100,
    },
  ],
  routes: [],
  translations: {},
};
