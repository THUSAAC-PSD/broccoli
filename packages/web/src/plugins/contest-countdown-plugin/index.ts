import type { ActivePluginManifest } from '@broccoli/web-sdk';

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
      name: 'contest-overview.header',
      position: 'after',
      component: 'contest/Countdown',
      priority: 100,
    },
    {
      name: 'problem-overview.content',
      position: 'before',
      component: 'contest/CountdownMini',
      priority: 100,
    },
  ],
  routes: [],
  translations: {},
};
