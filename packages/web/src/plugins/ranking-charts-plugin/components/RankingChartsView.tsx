import { RankChart, type ScoreSnapshot } from './RankChart';
import { ScoreDistribution, type DistributionEntry } from './ScoreDistribution';

interface RankingChartsViewProps {
  data?: ScoreSnapshot[];
  teams?: string[];
  distribution?: DistributionEntry[];
}

export function RankingChartsView({
  data = [],
  teams = [],
  distribution = [],
}: RankingChartsViewProps) {
  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <RankChart data={data} teams={teams} />
      <ScoreDistribution data={distribution} />
    </div>
  );
}
