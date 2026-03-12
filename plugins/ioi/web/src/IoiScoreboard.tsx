import { useTranslation } from '@broccoli/sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import type React from 'react';
import { useState } from 'react';

import { useIoiApi } from './hooks/useIoiApi';
import { useIsIoiContest } from './hooks/useIsIoiContest';
import type { ScoreboardEntry, ScoreboardResponse } from './types';

interface IoiScoreboardProps {
  contestId?: number;
}

const MEDAL_COLORS = ['#D4AF37', '#A8A8A8', '#CD7F32'] as const;

const SCORE_FONT: React.CSSProperties = {
  fontVariantNumeric: 'tabular-nums',
  fontFamily:
    'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
};

function scoreColor(score: number, maxPossible: number): string {
  if (maxPossible <= 0) return 'transparent';
  const frac = score / maxPossible;
  if (frac >= 1) return 'rgba(16, 185, 129, 0.25)';
  if (frac > 0) return `rgba(245, 158, 11, ${0.08 + frac * 0.17})`;
  return 'transparent';
}

function scoreBorderColor(score: number, maxPossible: number): string {
  if (maxPossible <= 0) return 'transparent';
  const frac = score / maxPossible;
  if (frac >= 1) return 'rgba(16, 185, 129, 0.5)';
  if (frac > 0) return 'rgba(245, 158, 11, 0.3)';
  return 'transparent';
}

function PhaseBar({ phase }: { phase: string }) {
  const { t } = useTranslation();
  const style: React.CSSProperties = {
    padding: '8px 16px',
    borderRadius: 6,
    fontSize: 13,
    fontWeight: 500,
    display: 'flex',
    alignItems: 'center',
    gap: 8,
  };

  if (phase === 'before') {
    return (
      <div
        style={{
          ...style,
          background: 'rgba(59, 130, 246, 0.1)',
          color: 'rgb(59, 130, 246)',
        }}
      >
        <span
          style={{
            width: 8,
            height: 8,
            borderRadius: '50%',
            background: 'rgb(59, 130, 246)',
          }}
        />
        {t('ioi.scoreboard.phase.before')}
      </div>
    );
  }
  if (phase === 'during') {
    return (
      <div
        style={{
          ...style,
          background: 'rgba(16, 185, 129, 0.1)',
          color: 'rgb(16, 185, 129)',
        }}
      >
        <span
          style={{
            width: 8,
            height: 8,
            borderRadius: '50%',
            background: 'rgb(16, 185, 129)',
          }}
        />
        {t('ioi.scoreboard.phase.during')}
      </div>
    );
  }
  return (
    <div
      style={{
        ...style,
        background: 'rgba(107, 114, 128, 0.1)',
        color: 'rgb(107, 114, 128)',
      }}
    >
      <span
        style={{
          width: 8,
          height: 8,
          borderRadius: '50%',
          background: 'rgb(107, 114, 128)',
        }}
      />
      {t('ioi.scoreboard.phase.after')}
    </div>
  );
}

function MedalBadge({ rank }: { rank: number }) {
  if (rank > 3) {
    return (
      <span
        style={{
          ...SCORE_FONT,
          fontSize: 13,
          color: 'var(--muted-foreground, #888)',
        }}
      >
        {rank}
      </span>
    );
  }
  const color = MEDAL_COLORS[rank - 1];
  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        width: 24,
        height: 24,
        borderRadius: '50%',
        background: color,
        color: '#fff',
        fontSize: 12,
        fontWeight: 700,
      }}
    >
      {rank}
    </span>
  );
}

function ScoreCell({ score, max }: { score: number; max: number }) {
  return (
    <td
      style={{
        padding: '6px 12px',
        textAlign: 'center',
        background: scoreColor(score, max),
        borderBottom: '1px solid var(--border, #e5e7eb)',
        position: 'relative',
      }}
    >
      <div
        style={{
          position: 'absolute',
          left: 0,
          top: 0,
          bottom: 0,
          width: 3,
          background: scoreBorderColor(score, max),
        }}
      />
      <span
        style={{
          ...SCORE_FONT,
          fontSize: 13,
          fontWeight: score > 0 ? 600 : 400,
          color:
            score > 0
              ? 'var(--foreground, #111)'
              : 'var(--muted-foreground, #999)',
        }}
      >
        {score.toFixed(score === Math.floor(score) ? 0 : 2)}
      </span>
    </td>
  );
}

export function IoiScoreboard({ contestId }: IoiScoreboardProps) {
  const { isIoi, isLoading: guardLoading } = useIsIoiContest(contestId);
  const api = useIoiApi();
  const { t } = useTranslation();
  const [autoRefresh, setAutoRefresh] = useState(true);

  const { data: scoreboard, isLoading } = useQuery<ScoreboardResponse>({
    queryKey: ['ioi-scoreboard', contestId],
    enabled: !!contestId && isIoi,
    queryFn: () => api.getScoreboard(contestId!),
    refetchInterval: (query: { state: { data?: ScoreboardResponse } }) =>
      autoRefresh && query.state.data?.phase === 'during' ? 30000 : false,
  });

  if (guardLoading || !isIoi) return null;

  if (isLoading || !scoreboard) {
    return (
      <div
        style={{
          padding: 24,
          textAlign: 'center',
          color: 'var(--muted-foreground, #888)',
        }}
      >
        {t('ioi.scoreboard.loading')}
      </div>
    );
  }

  const { phase, rankings } = scoreboard;

  // Determine problem labels from the first entry that has problems
  const sampleEntry = rankings.find(
    (r: ScoreboardEntry) => r.problems && r.problems.length > 0,
  );
  const problemIds =
    sampleEntry?.problems?.map((p: { problem_id: number }) => p.problem_id) ??
    [];
  const problemLabels = problemIds.map((_: number, i: number) =>
    String.fromCharCode(65 + i),
  );

  // Per-problem max scores from backend (sum of subtask max_scores or test case scores)
  const maxPerProblem: Record<number, number> = {};
  for (const pid of problemIds) {
    maxPerProblem[pid] = scoreboard.max_scores?.[String(pid)] ?? 100;
  }
  const maxTotal =
    Object.values(maxPerProblem).reduce((sum, v) => sum + v, 0) || 1;

  const headerStyle: React.CSSProperties = {
    padding: '8px 12px',
    textAlign: 'center',
    fontWeight: 600,
    fontSize: 12,
    textTransform: 'uppercase',
    letterSpacing: '0.05em',
    color: 'var(--muted-foreground, #888)',
    borderBottom: '2px solid var(--border, #e5e7eb)',
    background: 'var(--muted, #f9fafb)',
    whiteSpace: 'nowrap',
  };

  const cellStyle: React.CSSProperties = {
    padding: '6px 12px',
    borderBottom: '1px solid var(--border, #e5e7eb)',
    fontSize: 13,
  };

  return (
    <div>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          marginBottom: 12,
        }}
      >
        <PhaseBar phase={phase} />
        {phase === 'during' && (
          <label
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 6,
              fontSize: 12,
              color: 'var(--muted-foreground, #888)',
              cursor: 'pointer',
            }}
          >
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
              style={{ accentColor: 'rgb(16, 185, 129)' }}
            />
            {t('ioi.scoreboard.autoRefresh')}
          </label>
        )}
      </div>

      {phase === 'during' && (
        <div
          style={{
            padding: '6px 12px',
            marginBottom: 12,
            borderRadius: 4,
            fontSize: 12,
            color: 'var(--muted-foreground, #888)',
            background: 'var(--muted, #f9fafb)',
            border: '1px solid var(--border, #e5e7eb)',
          }}
        >
          {t('ioi.scoreboard.ownScoresOnly')}
        </div>
      )}

      {rankings.length === 0 ? (
        <div
          style={{
            padding: 48,
            textAlign: 'center',
            color: 'var(--muted-foreground, #888)',
          }}
        >
          {t('ioi.scoreboard.noRankings')}
        </div>
      ) : (
        <div
          style={{
            overflowX: 'auto',
            borderRadius: 8,
            border: '1px solid var(--border, #e5e7eb)',
          }}
        >
          <table
            style={{ width: '100%', borderCollapse: 'collapse', minWidth: 500 }}
          >
            <thead>
              <tr>
                <th style={{ ...headerStyle, width: 50, textAlign: 'center' }}>
                  {t('ioi.scoreboard.header.rank')}
                </th>
                <th style={{ ...headerStyle, textAlign: 'left' }}>
                  {t('ioi.scoreboard.header.user')}
                </th>
                {problemLabels.map((label: string, i: number) => (
                  <th key={problemIds[i]} style={{ ...headerStyle, width: 80 }}>
                    {label}
                  </th>
                ))}
                <th style={{ ...headerStyle, width: 90 }}>
                  {t('ioi.scoreboard.header.total')}
                </th>
              </tr>
            </thead>
            <tbody>
              {rankings.map((entry: ScoreboardEntry) => (
                <RankRow
                  key={entry.user_id}
                  entry={entry}
                  problemIds={problemIds}
                  maxPerProblem={maxPerProblem}
                  maxTotal={maxTotal}
                  cellStyle={cellStyle}
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
  problemIds,
  maxPerProblem,
  maxTotal,
  cellStyle,
}: {
  entry: ScoreboardEntry;
  problemIds: number[];
  maxPerProblem: Record<number, number>;
  maxTotal: number;
  cellStyle: React.CSSProperties;
}) {
  const problemScoreMap: Record<number, number> = {};
  if (entry.problems) {
    for (const p of entry.problems) {
      problemScoreMap[p.problem_id] = p.score;
    }
  }

  return (
    <tr style={{ transition: 'background 0.15s' }}>
      <td style={{ ...cellStyle, textAlign: 'center', width: 50 }}>
        <MedalBadge rank={entry.rank} />
      </td>
      <td style={{ ...cellStyle, fontWeight: 500 }}>{entry.username}</td>
      {problemIds.map((pid) => {
        const score = problemScoreMap[pid] ?? 0;
        const max = maxPerProblem[pid] ?? 100;
        return <ScoreCell key={pid} score={score} max={max} />;
      })}
      <td
        style={{
          ...cellStyle,
          textAlign: 'center',
          fontWeight: 700,
          background: scoreColor(entry.total_score, maxTotal),
          ...SCORE_FONT,
        }}
      >
        {entry.total_score.toFixed(
          entry.total_score === Math.floor(entry.total_score) ? 0 : 2,
        )}
      </td>
    </tr>
  );
}
