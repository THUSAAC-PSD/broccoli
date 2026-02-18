import type { ComponentBundle, PluginManifest } from '@broccoli/sdk';

import { RankChart } from './components/RankChart';
import { RankingChartsView } from './components/RankingChartsView';
import { ScoreDistribution } from './components/ScoreDistribution';

export const manifest: PluginManifest = {
  name: 'ranking-charts-plugin',
  version: '1.0.0',
  description: 'Charts and graphs for ranking visualization',
  author: 'Broccoli Team',
  enabled: true,
  slots: [
    {
      name: 'ranking.charts',
      position: 'replace',
      component: 'charts/RankingChartsView',
      priority: 50,
    },
  ],
};

export const components: ComponentBundle = {
  'charts/RankChart': RankChart,
  'charts/ScoreDistribution': ScoreDistribution,
  'charts/RankingChartsView': RankingChartsView,
};
