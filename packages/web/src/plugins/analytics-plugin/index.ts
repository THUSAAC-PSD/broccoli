import type { PluginManifest, ComponentBundle } from '@broccoli/sdk';
import { AnalyticsTracker } from './components/AnalyticsTracker';
import { PerformanceMonitor } from './components/PerformanceMonitor';

export const manifest: PluginManifest = {
  name: 'analytics-plugin',
  version: '1.0.0',
  description: 'Analytics and performance monitoring',
  author: 'Broccoli Team',
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
      condition: () => import.meta.env.DEV,
    },
  ],
  onInit: async () => {
    console.log('[Analytics Plugin] Initialized');
  },
  onDestroy: async () => {
    console.log('[Analytics Plugin] Destroyed');
  },
  enabled: true,
};

export const components: ComponentBundle = {
  'analytics/AnalyticsTracker': AnalyticsTracker,
  'analytics/PerformanceMonitor': PerformanceMonitor,
};
