import { useTranslation } from '@broccoli/sdk/i18n';
import { useParams } from 'react-router';

import { useIsIoiContest } from './hooks/useIsIoiContest';
import type { ScoringMode } from './types';

const SCORING_MODE_KEYS: Record<
  ScoringMode,
  { title: string; description: string }
> = {
  max_submission: {
    title: 'ioi.contestInfo.scoringMode.maxSubmission.title',
    description: 'ioi.contestInfo.scoringMode.maxSubmission.description',
  },
  sum_best_subtask: {
    title: 'ioi.contestInfo.scoringMode.sumBestSubtask.title',
    description: 'ioi.contestInfo.scoringMode.sumBestSubtask.description',
  },
  best_tokened_or_last: {
    title: 'ioi.contestInfo.scoringMode.bestTokenedOrLast.title',
    description: 'ioi.contestInfo.scoringMode.bestTokenedOrLast.description',
  },
};

const FEEDBACK_KEYS: Record<string, string> = {
  full: 'ioi.contestInfo.feedback.full',
  subtask_scores: 'ioi.contestInfo.feedback.subtaskScores',
  total_only: 'ioi.contestInfo.feedback.totalOnly',
  none: 'ioi.contestInfo.feedback.none',
  tokened_full: 'ioi.contestInfo.feedback.tokenedFull',
};

export function IoiContestInfo() {
  const { contestId } = useParams();
  const cId = contestId ? Number(contestId) : undefined;
  const { isIoi, contestInfo, isLoading } = useIsIoiContest(cId);
  const { t } = useTranslation();

  if (isLoading || !isIoi || !contestInfo) return null;

  const modeKeys = SCORING_MODE_KEYS[contestInfo.scoring_mode as ScoringMode];
  const mode = modeKeys
    ? { title: t(modeKeys.title), description: t(modeKeys.description) }
    : { title: contestInfo.scoring_mode, description: '' };

  return (
    <div
      style={{
        border: '1px solid var(--border, #e5e7eb)',
        borderRadius: 8,
        padding: '14px 18px',
        background: 'var(--card, #fff)',
        marginBottom: 4,
      }}
    >
      {/* Title row */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          marginBottom: 8,
        }}
      >
        <span
          style={{
            display: 'inline-flex',
            padding: '2px 8px',
            borderRadius: 4,
            fontSize: 11,
            fontWeight: 700,
            letterSpacing: '0.05em',
            background: 'rgba(16, 185, 129, 0.1)',
            color: '#10b981',
            textTransform: 'uppercase',
          }}
        >
          IOI
        </span>
        <span
          style={{
            fontSize: 14,
            fontWeight: 600,
            color: 'var(--foreground, #111)',
          }}
        >
          {mode.title}
        </span>
      </div>

      {/* Description */}
      <div
        style={{
          fontSize: 12,
          color: 'var(--muted-foreground, #888)',
          marginBottom: 10,
        }}
      >
        {mode.description}
      </div>

      {/* Metadata row */}
      <div
        style={{
          display: 'flex',
          flexWrap: 'wrap',
          gap: 16,
          fontSize: 12,
          color: 'var(--muted-foreground, #666)',
        }}
      >
        <MetaItem
          label={
            FEEDBACK_KEYS[contestInfo.feedback_level]
              ? t(FEEDBACK_KEYS[contestInfo.feedback_level])
              : contestInfo.feedback_level
          }
        />
        {contestInfo.token_mode !== 'none' && (
          <MetaItem
            label={
              contestInfo.token_mode === 'fixed_budget'
                ? t('ioi.contestInfo.tokenMode.fixedBudget')
                : t('ioi.contestInfo.tokenMode.regenerating')
            }
          />
        )}
      </div>
    </div>
  );
}

function MetaItem({ label }: { label: string }) {
  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: 4,
        padding: '2px 8px',
        borderRadius: 4,
        background: 'var(--muted, #f3f4f6)',
        fontSize: 11,
        fontWeight: 500,
      }}
    >
      {label}
    </span>
  );
}
