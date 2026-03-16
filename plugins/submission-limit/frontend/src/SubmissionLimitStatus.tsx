/**
 * Shows submission count / limit on the problem detail sidebar.
 */
import { useApiFetch } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { cn } from '@broccoli/web-sdk/utils';
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

const PLUGIN_BASE = '/api/v1/p/submission-limit/api/plugins/submission-limit';

export function SubmissionLimitStatus({
  submission,
  contestId,
  problemId,
}: Props) {
  const apiFetch = useApiFetch();
  const { t } = useTranslation();
  const [status, setStatus] = useState<LimitStatus | null>(null);

  const submissionId = submission?.id;

  useEffect(() => {
    if (!problemId) return;

    let cancelled = false;

    async function load() {
      const url = contestId
        ? `${PLUGIN_BASE}/contests/${contestId}/problems/${problemId}/status`
        : `${PLUGIN_BASE}/problems/${problemId}/status`;
      try {
        const res = await apiFetch(url);
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
  }, [apiFetch, contestId, problemId, submissionId]);

  if (!problemId || !status) return null;

  const { submissions_made, max_submissions, unlimited, remaining } = status;

  const pct = unlimited
    ? 0
    : Math.min((submissions_made / max_submissions) * 100, 100);

  return (
    <div className="rounded-lg border border-border p-4 bg-card">
      <div className="text-xs font-semibold uppercase tracking-wide text-muted-foreground mb-3">
        {t('limit.submissions')}
      </div>

      {unlimited ? (
        <div className="font-mono tabular-nums text-[13px] text-foreground">
          {t('limit.submittedNoLimit', { count: submissions_made })}
        </div>
      ) : (
        <>
          <div className="flex justify-between items-baseline mb-1.5">
            <span
              className={cn(
                'font-mono tabular-nums text-[13px]',
                !unlimited && remaining === 0
                  ? 'text-red-500'
                  : 'text-foreground',
              )}
            >
              {submissions_made} / {max_submissions}
            </span>
            {remaining !== null && remaining > 0 && (
              <span className="text-[11px] text-muted-foreground">
                {t('limit.remaining', { count: remaining })}
              </span>
            )}
            {remaining === 0 && (
              <span className="text-[11px] text-red-500 font-medium">
                {t('limit.reached')}
              </span>
            )}
          </div>
          <div className="h-1 rounded-sm bg-muted overflow-hidden">
            <div
              className={cn(
                'h-full rounded-sm transition-[width] duration-300 ease-out',
                !unlimited && remaining === 0
                  ? 'bg-red-500'
                  : !unlimited &&
                      remaining !== null &&
                      remaining <= Math.ceil(max_submissions * 0.1)
                    ? 'bg-amber-500'
                    : 'bg-primary',
              )}
              style={{ width: `${pct}%` }}
            />
          </div>
        </>
      )}
    </div>
  );
}
