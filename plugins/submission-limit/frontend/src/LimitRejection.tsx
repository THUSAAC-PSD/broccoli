/**
 * Submission-limit rejection wrapper for the `submission-result.rejection` slot.
 */
import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { ReactNode } from 'react';

interface Props {
  error?: { code: string; message: string; details?: Record<string, unknown> };
  children?: ReactNode;
}

const MONO: React.CSSProperties = {
  fontVariantNumeric: 'tabular-nums',
  fontFamily:
    'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
};

function getLimitCounts(
  details?: Record<string, unknown>,
): { used: number; total: number } | null {
  if (!details) return null;
  const used = details.submissions_made;
  const total = details.max_submissions;
  if (typeof used === 'number' && typeof total === 'number') {
    return { used, total };
  }
  return null;
}

export function LimitRejection({ error, children }: Props) {
  const { t } = useTranslation();

  if (!error || error.code !== 'SUBMISSION_LIMIT_EXCEEDED') {
    return <>{children}</>;
  }

  const counts = getLimitCounts(error.details);

  return (
    <div
      style={{
        border: '1px solid var(--border, #e5e7eb)',
        borderRadius: 12,
        background: 'var(--card, #fff)',
        overflow: 'hidden',
        height: '100%',
      }}
    >
      {/* Header */}
      <div style={{ padding: '20px 24px 0' }}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <span
            style={{
              fontSize: 16,
              fontWeight: 600,
              color: 'var(--foreground, #111)',
            }}
          >
            {t('limit.result')}
          </span>
          <span
            style={{
              fontSize: 11,
              fontWeight: 500,
              padding: '2px 8px',
              borderRadius: 9999,
              background: '#fef2f2',
              color: '#ef4444',
            }}
          >
            {t('limit.limitReached')}
          </span>
        </div>
      </div>

      {/* Content */}
      <div
        style={{
          padding: '8px 24px 24px',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          gap: 20,
        }}
      >
        {/* Icon */}
        <div
          style={{
            width: 64,
            height: 64,
            borderRadius: '50%',
            background: '#fef2f2',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <svg
            width="32"
            height="32"
            viewBox="0 0 24 24"
            fill="none"
            stroke="#ef4444"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <circle cx="12" cy="12" r="10" />
            <line x1="15" y1="9" x2="9" y2="15" />
            <line x1="9" y1="9" x2="15" y2="15" />
          </svg>
        </div>

        {/* Message */}
        <div style={{ textAlign: 'center', maxWidth: 280 }}>
          <p
            style={{
              fontSize: 13,
              fontWeight: 500,
              color: 'var(--foreground, #111)',
            }}
          >
            {t('limit.submissionLimitReached')}
          </p>
          <p
            style={{
              fontSize: 11,
              color: 'var(--muted-foreground, #888)',
              marginTop: 4,
            }}
          >
            {t('limit.allUsed')}
          </p>
        </div>

        {/* Progress bar with count */}
        {counts && (
          <div style={{ width: '100%', maxWidth: 240 }}>
            <div
              style={{
                height: 8,
                borderRadius: 4,
                background: 'var(--muted, #f3f4f6)',
                overflow: 'hidden',
              }}
            >
              <div
                style={{
                  height: '100%',
                  borderRadius: 4,
                  background: '#ef4444',
                  width: '100%',
                }}
              />
            </div>
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                marginTop: 8,
              }}
            >
              <span
                style={{
                  ...MONO,
                  fontSize: 11,
                  color: 'var(--muted-foreground, #888)',
                }}
              >
                {t('limit.usedCount', {
                  used: counts.used,
                  total: counts.total,
                })}
              </span>
              <span
                style={{
                  ...MONO,
                  fontSize: 11,
                  fontWeight: 500,
                  color: '#ef4444',
                }}
              >
                {t('limit.zeroRemaining')}
              </span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
