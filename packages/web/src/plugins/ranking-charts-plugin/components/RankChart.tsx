import {
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';

import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { useTheme } from '@/hooks/use-theme';

const COLORS = [
  '#3b82f6', // blue
  '#ef4444', // red
  '#22c55e', // green
  '#f59e0b', // amber
  '#8b5cf6', // violet
  '#ec4899', // pink
  '#06b6d4', // cyan
  '#f97316', // orange
];

export interface ScoreSnapshot {
  time: number; // minutes from start
  [teamName: string]: number;
}

interface RankChartProps {
  data: ScoreSnapshot[];
  teams: string[];
}

export function RankChart({ data, teams }: RankChartProps) {
  const { theme } = useTheme();
  const isDark = theme === 'dark';

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-base">Score Over Time</CardTitle>
      </CardHeader>
      <CardContent>
        <ResponsiveContainer width="100%" height={300}>
          <LineChart data={data}>
            <CartesianGrid
              strokeDasharray="3 3"
              stroke={isDark ? '#374151' : '#e5e7eb'}
            />
            <XAxis
              dataKey="time"
              tick={{ fontSize: 12, fill: isDark ? '#9ca3af' : '#6b7280' }}
              tickFormatter={(v) => `${v}m`}
              stroke={isDark ? '#4b5563' : '#d1d5db'}
            />
            <YAxis
              tick={{ fontSize: 12, fill: isDark ? '#9ca3af' : '#6b7280' }}
              stroke={isDark ? '#4b5563' : '#d1d5db'}
            />
            <Tooltip
              contentStyle={{
                backgroundColor: isDark ? '#1f2937' : '#fff',
                border: `1px solid ${isDark ? '#374151' : '#e5e7eb'}`,
                borderRadius: '8px',
                fontSize: 12,
                color: isDark ? '#f3f4f6' : '#111827',
              }}
              labelFormatter={(v) => `${v} min`}
            />
            <Legend wrapperStyle={{ fontSize: 12 }} />
            {teams.map((team, i) => (
              <Line
                key={team}
                type="monotone"
                dataKey={team}
                stroke={COLORS[i % COLORS.length]}
                strokeWidth={2}
                dot={false}
                activeDot={{ r: 4 }}
              />
            ))}
          </LineChart>
        </ResponsiveContainer>
      </CardContent>
    </Card>
  );
}
