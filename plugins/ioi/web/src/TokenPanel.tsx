import { useTranslation } from '@broccoli/sdk/i18n';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import type React from 'react';
import { useState } from 'react';

import { useIoiApi } from './hooks/useIoiApi';
import { useIsIoiContest } from './hooks/useIsIoiContest';

interface TokenPanelProps {
  submission?: {
    id: number;
    status: string;
    contest_id?: number | null;
  } | null;
  contestId?: number;
}

const SCORE_FONT: React.CSSProperties = {
  fontVariantNumeric: 'tabular-nums',
  fontFamily:
    'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
};

export function TokenPanel({ submission, contestId }: TokenPanelProps) {
  const cId = contestId ?? submission?.contest_id ?? undefined;
  const { isIoi, contestInfo, isLoading: guardLoading } = useIsIoiContest(cId);
  const api = useIoiApi();
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const [isUsing, setIsUsing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmingSubmissionId, setConfirmingSubmissionId] = useState<
    number | null
  >(null);

  const tokenMode = contestInfo?.token_mode;
  const scoringMode = contestInfo?.scoring_mode;
  const showTokens =
    isIoi &&
    tokenMode &&
    tokenMode !== 'none' &&
    scoringMode === 'best_tokened_or_last';

  const { data: tokenStatus } = useQuery({
    queryKey: ['ioi-token-status', cId],
    enabled: !!cId && showTokens === true,
    queryFn: () => api.getTokenStatus(cId!),
    refetchInterval: 60000,
  });

  if (guardLoading || !isIoi || !showTokens) return null;
  if (!tokenStatus) return null;

  const canUseToken =
    submission &&
    submission.status === 'Judged' &&
    tokenStatus.available > 0 &&
    !tokenStatus.tokened_submission_ids.includes(submission.id);

  const alreadyTokened =
    submission && tokenStatus.tokened_submission_ids.includes(submission.id);

  const handleUseToken = async () => {
    if (!submission || !cId) return;
    setIsUsing(true);
    setError(null);
    try {
      await api.useToken(cId, submission.id);
      queryClient.invalidateQueries({ queryKey: ['ioi-token-status', cId] });
      queryClient.invalidateQueries({
        queryKey: ['ioi-subtask-scores', cId, submission.id],
      });
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Failed to use token');
    } finally {
      setIsUsing(false);
    }
  };

  const dots = [];
  const showDots = tokenStatus.total <= 20;
  if (showDots) {
    for (let i = 0; i < tokenStatus.total; i++) {
      const isAvailable = i < tokenStatus.available;
      dots.push(
        <span
          key={i}
          style={{
            display: 'inline-block',
            width: 10,
            height: 10,
            borderRadius: '50%',
            background: isAvailable ? '#10b981' : 'transparent',
            border: isAvailable
              ? '2px solid #10b981'
              : '2px solid var(--border, #d1d5db)',
            transition: 'all 0.2s',
          }}
        />,
      );
    }
  }

  return (
    <div
      style={{
        border: '1px solid var(--border, #e5e7eb)',
        borderRadius: 8,
        padding: 16,
        background: 'var(--card, #fff)',
      }}
    >
      <div
        style={{
          fontSize: 12,
          fontWeight: 600,
          textTransform: 'uppercase',
          letterSpacing: '0.05em',
          color: 'var(--muted-foreground, #888)',
          marginBottom: 12,
        }}
      >
        {t('ioi.tokenPanel.title')}
      </div>

      {/* Token display */}
      {tokenStatus.total > 0 && (
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 4,
            marginBottom: 8,
            flexWrap: 'wrap',
          }}
        >
          {showDots ? (
            dots
          ) : (
            <span
              style={{
                ...SCORE_FONT,
                fontSize: 20,
                fontWeight: 700,
                color: 'var(--foreground, #111)',
              }}
            >
              {tokenStatus.available}
              <span style={{ opacity: 0.4 }}>/{tokenStatus.total}</span>
            </span>
          )}
          {showDots && (
            <span
              style={{
                ...SCORE_FONT,
                marginLeft: 8,
                fontSize: 13,
                color: 'var(--foreground, #111)',
              }}
            >
              {t('ioi.tokenPanel.available', {
                available: tokenStatus.available,
                total: tokenStatus.total,
              })}
            </span>
          )}
        </div>
      )}

      {/* Use token button with confirmation */}
      {canUseToken && (
        <div style={{ position: 'relative' }}>
          {confirmingSubmissionId === submission.id ? (
            <div
              style={{
                padding: '10px 12px',
                borderRadius: 6,
                border: '1px solid rgba(245, 158, 11, 0.3)',
                background: 'rgba(245, 158, 11, 0.06)',
                display: 'flex',
                flexDirection: 'column',
                gap: 8,
              }}
            >
              <div
                style={{
                  fontSize: 12,
                  color: 'var(--foreground, #111)',
                  lineHeight: 1.5,
                }}
              >
                {t('ioi.tokenPanel.confirmUse', {
                  id: submission.id,
                  remaining: tokenStatus.available - 1,
                })}
              </div>
              <div style={{ display: 'flex', gap: 8 }}>
                <button
                  onClick={() => {
                    setConfirmingSubmissionId(null);
                    handleUseToken();
                  }}
                  disabled={isUsing}
                  style={{
                    flex: 1,
                    padding: '6px 12px',
                    borderRadius: 5,
                    border: '1px solid var(--primary, #3b82f6)',
                    background: 'var(--primary, #3b82f6)',
                    color: '#fff',
                    fontSize: 12,
                    fontWeight: 600,
                    cursor: isUsing ? 'not-allowed' : 'pointer',
                    opacity: isUsing ? 0.6 : 1,
                  }}
                >
                  {isUsing
                    ? t('ioi.tokenPanel.usingToken')
                    : t('ioi.tokenPanel.confirm')}
                </button>
                <button
                  onClick={() => setConfirmingSubmissionId(null)}
                  style={{
                    flex: 1,
                    padding: '6px 12px',
                    borderRadius: 5,
                    border: '1px solid var(--border, #d1d5db)',
                    background: 'transparent',
                    color: 'var(--foreground, #111)',
                    fontSize: 12,
                    fontWeight: 500,
                    cursor: 'pointer',
                  }}
                >
                  {t('ioi.tokenPanel.cancel')}
                </button>
              </div>
            </div>
          ) : (
            <button
              onClick={() => setConfirmingSubmissionId(submission.id)}
              style={{
                width: '100%',
                padding: '8px 16px',
                borderRadius: 6,
                border: '1px solid var(--primary, #3b82f6)',
                background: 'var(--primary, #3b82f6)',
                color: '#fff',
                fontSize: 13,
                fontWeight: 600,
                cursor: 'pointer',
                transition: 'opacity 0.15s',
              }}
            >
              {t('ioi.tokenPanel.useToken')}
            </button>
          )}
        </div>
      )}

      {alreadyTokened && (
        <div
          style={{
            padding: '6px 10px',
            borderRadius: 4,
            fontSize: 12,
            color: '#10b981',
            background: 'rgba(16, 185, 129, 0.1)',
            textAlign: 'center',
          }}
        >
          {t('ioi.tokenPanel.alreadyUsed')}
        </div>
      )}

      {error && (
        <div
          style={{
            marginTop: 8,
            padding: '6px 10px',
            borderRadius: 4,
            fontSize: 12,
            color: '#ef4444',
            background: 'rgba(239, 68, 68, 0.1)',
          }}
        >
          {error}
        </div>
      )}

      {/* Tokened submissions list */}
      {tokenStatus.tokened_submission_ids.length > 0 && (
        <div style={{ marginTop: 12 }}>
          <div
            style={{
              fontSize: 11,
              color: 'var(--muted-foreground, #888)',
              marginBottom: 4,
            }}
          >
            {t('ioi.tokenPanel.tokenedSubmissions')}
          </div>
          <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
            {tokenStatus.tokened_submission_ids.map((sid: number) => (
              <span
                key={sid}
                style={{
                  ...SCORE_FONT,
                  padding: '2px 8px',
                  borderRadius: 4,
                  fontSize: 11,
                  background: 'var(--muted, #f3f4f6)',
                  color: 'var(--foreground, #111)',
                }}
              >
                #{sid}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
