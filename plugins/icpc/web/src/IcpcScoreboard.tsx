import { Label, Switch } from '@broccoli/web-sdk/ui';
import { cn } from '@broccoli/web-sdk/utils';
import { useQuery } from '@tanstack/react-query';
import { type ReactNode, useEffect, useState } from 'react';

import { useIcpcApi } from './hooks/useIcpcApi';
import { useIsIcpcContest } from './hooks/useIsIcpcContest';
import type { ProblemCell, StandingsEntry, StandingsResponse } from './types';

interface IcpcScoreboardProps {
  contestId?: number;
  children?: ReactNode;
}

const MEDAL_COLORS = ['#D4AF37', '#A8A8A8', '#CD7F32'] as const;

function PhaseBar({ phase }: { phase: string }) {
  return (
    <div
      className={cn(
        'flex items-center gap-2 py-2 px-4 rounded-md text-[13px] font-medium',
        phase === 'before' && 'bg-blue-500/10 text-blue-500',
        phase === 'during' && 'bg-emerald-500/10 text-emerald-500',
        phase !== 'before' &&
          phase !== 'during' &&
          'bg-gray-500/10 text-gray-500',
      )}
    >
      <span
        className={cn(
          'w-2 h-2 rounded-full',
          phase === 'before'
            ? 'bg-blue-500'
            : phase === 'during'
              ? 'bg-emerald-500'
              : 'bg-gray-500',
        )}
      />
      {phase === 'before'
        ? 'Not started'
        : phase === 'during'
          ? 'In progress'
          : 'Finished'}
    </div>
  );
}

function MedalBadge({ rank }: { rank: number }) {
  if (rank > 3) {
    return (
      <span className="font-mono tabular-nums text-[13px] text-muted-foreground">
        {rank}
      </span>
    );
  }
  const color = MEDAL_COLORS[rank - 1];
  return (
    <span
      className="inline-flex items-center justify-center w-6 h-6 rounded-full text-white text-xs font-bold"
      style={{ background: color }}
    >
      {rank}
    </span>
  );
}

function ProblemCellView({ cell }: { cell: ProblemCell | undefined }) {
  if (!cell) {
    // No attempts
    return (
      <td className="py-1.5 px-2 text-center border-b border-border">
        <span className="text-[11px] text-muted-foreground/30">&mdash;</span>
      </td>
    );
  }

  if (cell.solved) {
    return (
      <td
        className={cn(
          'py-1.5 px-2 text-center border-b border-border',
          cell.first_solve ? 'bg-emerald-500/20' : 'bg-emerald-500/10',
        )}
      >
        <div
          className={cn(
            'font-mono text-[13px] leading-tight',
            cell.first_solve
              ? 'font-bold text-emerald-600'
              : 'font-semibold text-emerald-700',
          )}
        >
          {cell.attempts > 0 ? `+${cell.attempts}` : '+'}
        </div>
        <div className="font-mono text-[10px] text-muted-foreground">
          {cell.time}
        </div>
      </td>
    );
  }

  // Attempted but unsolved
  return (
    <td className="py-1.5 px-2 text-center border-b border-border bg-red-500/8">
      <div className="font-mono text-[13px] font-semibold text-red-600 leading-tight">
        -{cell.attempts}
      </div>
    </td>
  );
}

export function IcpcScoreboard({ contestId, children }: IcpcScoreboardProps) {
  const { isIcpc, isLoading: guardLoading } = useIsIcpcContest(contestId);
  const api = useIcpcApi();
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [now, setNow] = useState(Date.now());

  const {
    data: standings,
    isLoading,
    isError,
    dataUpdatedAt,
  } = useQuery<StandingsResponse>({
    queryKey: ['icpc-standings', contestId],
    enabled: !!contestId && isIcpc,
    queryFn: () => api.getStandings(contestId!),
    retry: 2,
    refetchInterval: (query: { state: { data?: StandingsResponse } }) =>
      autoRefresh && query.state.data?.phase === 'during' ? 30000 : false,
  });

  const hasData = dataUpdatedAt > 0;
  useEffect(() => {
    if (!hasData) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [hasData]);

  if (guardLoading || !isIcpc) return <>{children}</>;

  if (isError && !standings) {
    return (
      <div className="p-6 text-center rounded-md bg-red-500/[0.06] text-red-600 text-[13px]">
        Failed to load scoreboard
      </div>
    );
  }

  if (isLoading || !standings) {
    return (
      <div className="p-6 text-center text-muted-foreground">
        Loading scoreboard...
      </div>
    );
  }

  const { phase, problem_labels, rows } = standings;

  const secondsAgo =
    dataUpdatedAt > 0 ? Math.floor((now - dataUpdatedAt) / 1000) : 0;

  return (
    <div>
      <div className="flex items-center justify-between mb-3 flex-wrap gap-2">
        <PhaseBar phase={phase} />
        <div className="flex items-center gap-3">
          {dataUpdatedAt > 0 && (
            <span className="text-[11px] opacity-50">{secondsAgo}s ago</span>
          )}
          {phase === 'during' && (
            <Label className="flex items-center gap-1.5 text-xs text-muted-foreground cursor-pointer font-normal">
              <Switch checked={autoRefresh} onCheckedChange={setAutoRefresh} />
              Auto-refresh
            </Label>
          )}
        </div>
      </div>

      {rows.length === 0 ? (
        <div className="py-12 text-center text-muted-foreground">
          {phase === 'before'
            ? 'Contest has not started yet'
            : phase === 'during'
              ? 'No submissions yet'
              : 'No participants'}
        </div>
      ) : (
        <div className="overflow-x-auto rounded-lg border border-border">
          <table className="w-full border-collapse min-w-[500px]">
            <thead>
              <tr>
                <th
                  className="py-2 px-3 text-center font-semibold text-xs uppercase tracking-wide text-muted-foreground border-b-2 border-border bg-muted whitespace-nowrap"
                  style={{ width: 50 }}
                >
                  #
                </th>
                <th className="py-2 px-3 text-left font-semibold text-xs uppercase tracking-wide text-muted-foreground border-b-2 border-border bg-muted whitespace-nowrap">
                  Team
                </th>
                <th
                  className="py-2 px-3 text-center font-semibold text-xs uppercase tracking-wide text-muted-foreground border-b-2 border-border bg-muted whitespace-nowrap"
                  style={{ width: 60 }}
                >
                  =
                </th>
                <th
                  className="py-2 px-3 text-center font-semibold text-xs uppercase tracking-wide text-muted-foreground border-b-2 border-border bg-muted whitespace-nowrap"
                  style={{ width: 70 }}
                >
                  Penalty
                </th>
                {problem_labels.map((label) => (
                  <th
                    key={label}
                    className="py-2 px-2 text-center font-semibold text-xs uppercase tracking-wide text-muted-foreground border-b-2 border-border bg-muted whitespace-nowrap"
                    style={{ width: 65 }}
                  >
                    {label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((entry) => (
                <RankRow
                  key={entry.user_id}
                  entry={entry}
                  problemLabels={problem_labels}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function RankRow({
  entry,
  problemLabels,
}: {
  entry: StandingsEntry;
  problemLabels: string[];
}) {
  return (
    <tr className="transition-colors duration-150 hover:bg-muted/50">
      <td
        className="py-1.5 px-3 border-b border-border text-center"
        style={{ width: 50 }}
      >
        <MedalBadge rank={entry.rank} />
      </td>
      <td className="py-1.5 px-3 border-b border-border text-[13px] font-medium">
        {entry.username}
      </td>
      <td
        className="py-1.5 px-3 border-b border-border text-center font-bold font-mono tabular-nums text-[14px]"
        style={{ width: 60 }}
      >
        {entry.solved}
      </td>
      <td
        className="py-1.5 px-3 border-b border-border text-center font-mono tabular-nums text-[13px] text-muted-foreground"
        style={{ width: 70 }}
      >
        {entry.penalty}
      </td>
      {problemLabels.map((label) => (
        <ProblemCellView key={label} cell={entry.problems[label]} />
      ))}
    </tr>
  );
}
