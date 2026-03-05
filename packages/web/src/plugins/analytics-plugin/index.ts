import type { ActivePluginManifest } from '@broccoli/sdk';

import { AnalyticsTracker } from './components/AnalyticsTracker';
import { PerformanceMonitor } from './components/PerformanceMonitor';

export { AnalyticsTracker, PerformanceMonitor };

export const manifest: ActivePluginManifest = {
  id: 'analytics-plugin',
  name: 'analytics-plugin',
  entry: '',
  components: {
    'analytics/AnalyticsTracker': 'AnalyticsTracker',
    'analytics/PerformanceMonitor': 'PerformanceMonitor',
  },
  slots: [
    {
      name: 'app.root',
      position: 'wrap',
      component: 'analytics/AnalyticsTracker',
      priority: 100,
    },
    {
      name: 'sidebar.account.menu',
      position: 'append',
      component: 'analytics/PerformanceMonitor',
      priority: 0,
      // Only show in development
      // condition: () => import.meta.env.DEV,
      // TODO: move this condition to the component level
    },
  ],
  routes: [],
  translations: {},
};

export const onInit = async () => {
  console.log('[Analytics Plugin] Initialized');
};

export const onDestroy = async () => {
  console.log('[Analytics Plugin] Destroyed');
};
