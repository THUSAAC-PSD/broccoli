import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';

import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { useTheme } from '@/hooks/use-theme';

export interface DistributionEntry {
  solved: string; // e.g. "0", "1", "2"
  count: number;
}

interface ScoreDistributionProps {
  data: DistributionEntry[];
}

/** Generate a color for each bar based on its position in the gradient (red → green) */
function generateBarColor(index: number, total: number): string {
  // Hue from 0 (red) to 140 (green), with 0-solved getting a neutral gray
  if (index === 0) return '#94a3b8';
  const hue = (index / Math.max(total - 1, 1)) * 140;
  return `hsl(${hue}, 65%, 55%)`;
}

export function ScoreDistribution({ data }: ScoreDistributionProps) {
  const { theme } = useTheme();
  const isDark = theme === 'dark';

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-base">
          Problems Solved Distribution
        </CardTitle>
      </CardHeader>
      <CardContent>
        <ResponsiveContainer width="100%" height={300}>
          <BarChart data={data}>
            <CartesianGrid
              strokeDasharray="3 3"
              stroke={isDark ? '#374151' : '#e5e7eb'}
              vertical={false}
            />
            <XAxis
              dataKey="solved"
              tick={{ fontSize: 12, fill: isDark ? '#9ca3af' : '#6b7280' }}
              stroke={isDark ? '#4b5563' : '#d1d5db'}
              label={{
                value: 'Problems Solved',
                position: 'insideBottom',
                offset: -5,
                fontSize: 12,
                fill: isDark ? '#9ca3af' : '#6b7280',
              }}
            />
            <YAxis
              tick={{ fontSize: 12, fill: isDark ? '#9ca3af' : '#6b7280' }}
              stroke={isDark ? '#4b5563' : '#d1d5db'}
              allowDecimals={false}
              label={{
                value: 'Teams',
                angle: -90,
                position: 'insideLeft',
                fontSize: 12,
                fill: isDark ? '#9ca3af' : '#6b7280',
              }}
            />
            <Tooltip
              contentStyle={{
                backgroundColor: isDark ? '#1f2937' : '#fff',
                border: `1px solid ${isDark ? '#374151' : '#e5e7eb'}`,
                borderRadius: '8px',
                fontSize: 12,
                color: isDark ? '#f3f4f6' : '#111827',
              }}
              formatter={(value) => [`${value} teams`, 'Count']}
              labelFormatter={(label) =>
                `${label} problem${label === '1' ? '' : 's'} solved`
              }
            />
            <Bar dataKey="count" radius={[4, 4, 0, 0]} maxBarSize={48}>
              {data.map((_, index) => (
                <Cell
                  key={`cell-${index}`}
                  fill={generateBarColor(index, data.length)}
                  opacity={0.85}
                />
              ))}
            </Bar>
          </BarChart>
        </ResponsiveContainer>
      </CardContent>
    </Card>
  );
}
