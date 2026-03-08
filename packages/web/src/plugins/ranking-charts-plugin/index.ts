import type { ActivePluginManifest } from '@broccoli/web-sdk/plugin';

import { RankChart } from './components/RankChart';
import { RankingChartsView } from './components/RankingChartsView';
import { ScoreDistribution } from './components/ScoreDistribution';

export { RankChart, RankingChartsView, ScoreDistribution };

export const manifest: ActivePluginManifest = {
  id: 'ranking-charts-plugin',
  name: 'ranking-charts-plugin',
  entry: '',
  components: {
    'charts/RankChart': 'RankChart',
    'charts/ScoreDistribution': 'ScoreDistribution',
    'charts/RankingChartsView': 'RankingChartsView',
  },
  slots: [
    {
      name: 'ranking.charts',
      position: 'replace',
      component: 'charts/RankingChartsView',
      priority: 50,
    },
  ],
  routes: [],
  translations: {},
};
