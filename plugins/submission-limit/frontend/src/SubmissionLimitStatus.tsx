/**
 * Shows submission count / limit on the problem detail sidebar.
 */
import { useTranslation } from '@broccoli/sdk/i18n';
import { useEffect, useState } from 'react';

interface Props {
  submission?: { id: number; status: string } | null;
  contestId?: number;
  problemId?: number;
}

interface LimitStatus {
  submissions_made: number;
  max_submissions: number;
  remaining: number | null;
  unlimited: boolean;
}

const BACKEND_ORIGIN = new URL(import.meta.url).origin;
const AUTH_TOKEN_KEY = 'broccoli_token';

function authHeaders(): HeadersInit {
  const token = localStorage.getItem(AUTH_TOKEN_KEY);
  return token ? { Authorization: `Bearer ${token}` } : {};
}

const MONO: React.CSSProperties = {
  fontVariantNumeric: 'tabular-nums',
  fontFamily:
    'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
};

export function SubmissionLimitStatus({
  submission,
  contestId,
  problemId,
}: Props) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<LimitStatus | null>(null);

  const submissionId = submission?.id;

  useEffect(() => {
    if (!contestId || !problemId) return;

    let cancelled = false;

    async function load() {
      try {
        const res = await fetch(
          `${BACKEND_ORIGIN}/api/plugins/submission-limit/contests/${contestId}/problems/${problemId}/status`,
          { headers: authHeaders() },
        );
        if (!res.ok || cancelled) return;
        const data = await res.json();
        if (!cancelled) setStatus(data);
      } catch {
        // silent — status indicator is best-effort
      }
    }

    load();
    return () => {
      cancelled = true;
    };
  }, [contestId, problemId, submissionId]);

  if (!contestId || !problemId || !status) return null;

  const { submissions_made, max_submissions, unlimited, remaining } = status;

  // Determine color based on proximity to limit
  let barColor = 'var(--primary, #3b82f6)';
  let textColor = 'var(--foreground, #111)';
  if (!unlimited && remaining !== null) {
    if (remaining === 0) {
      barColor = '#ef4444';
      textColor = '#ef4444';
    } else if (remaining <= Math.ceil(max_submissions * 0.1)) {
      barColor = '#f59e0b';
    }
  }

  const pct = unlimited
    ? 0
    : Math.min((submissions_made / max_submissions) * 100, 100);

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
        {t('limit.submissions')}
      </div>

      {unlimited ? (
        <div
          style={{ ...MONO, fontSize: 13, color: 'var(--foreground, #111)' }}
        >
          {t('limit.submittedNoLimit', { count: submissions_made })}
        </div>
      ) : (
        <>
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'baseline',
              marginBottom: 6,
            }}
          >
            <span style={{ ...MONO, fontSize: 13, color: textColor }}>
              {submissions_made} / {max_submissions}
            </span>
            {remaining !== null && remaining > 0 && (
              <span
                style={{ fontSize: 11, color: 'var(--muted-foreground, #888)' }}
              >
                {t('limit.remaining', { count: remaining })}
              </span>
            )}
            {remaining === 0 && (
              <span style={{ fontSize: 11, color: '#ef4444', fontWeight: 500 }}>
                {t('limit.reached')}
              </span>
            )}
          </div>
          <div
            style={{
              height: 4,
              borderRadius: 2,
              background: 'var(--muted, #f3f4f6)',
              overflow: 'hidden',
            }}
          >
            <div
              style={{
                height: '100%',
                width: `${pct}%`,
                borderRadius: 2,
                background: barColor,
                transition: 'width 0.3s ease',
              }}
            />
          </div>
        </>
      )}
    </div>
  );
}
