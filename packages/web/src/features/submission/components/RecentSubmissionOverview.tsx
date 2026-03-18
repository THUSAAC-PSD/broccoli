import { useTranslation } from '@broccoli/web-sdk/i18n';
import type {
  SubmissionStatus,
  SubmissionSummary,
  Verdict,
} from '@broccoli/web-sdk/submission';
import { Badge } from '@broccoli/web-sdk/ui';
import { formatRelativeDatetime } from '@broccoli/web-sdk/utils';
import { useNavigate } from 'react-router';

import type { SubmissionEntry } from '@/features/submission/hooks/use-submissions';

import { getVerdictBadge } from '../utils/verdict';

const DEFAULT_VISIBLE_COUNT = 5;

interface RecentSubmissionOverviewProps {
  entries: SubmissionEntry[];
  history: SubmissionSummary[];
  isSubmitting?: boolean;
  visibleCount?: number;
  linkBuilder?: (submissionId: number) => string;
}

interface OverviewRow {
  key: string;
  submissionId: number | null;
  pendingOrder: number;
  status: SubmissionStatus;
  verdict: Verdict | null;
  score: number | null;
  createdAt: string | null;
  timeUsed: number | null;
  memoryUsed: number | null;
}

function getEntryStatus(entry: SubmissionEntry): SubmissionStatus {
  if (entry.submission) {
    return entry.submission.status;
  }

  switch (entry.status) {
    case 'submitting':
      return 'Pending';
    case 'polling':
      return 'Running';
    case 'error':
      return 'SystemError';
    default:
      return 'Pending';
  }
}

function formatScore(value: number): string {
  if (!Number.isFinite(value)) return String(value);
  const rounded = Math.round(value * 100) / 100;
  if (Number.isInteger(rounded)) return String(rounded);
  return rounded
    .toFixed(2)
    .replace(/\.0+$/, '')
    .replace(/(\.\d*[1-9])0+$/, '$1');
}

function getScoreToneClass(variant: string): string {
  switch (variant) {
    case 'accepted':
      return 'text-emerald-600 dark:text-emerald-300';
    case 'wronganswer':
    case 'runtimeerror':
      return 'text-rose-600 dark:text-rose-300';
    case 'timelimitexceeded':
    case 'memorylimitexceeded':
      return 'text-amber-600 dark:text-amber-300';
    case 'secondary':
      return 'text-slate-600 dark:text-slate-300';
    default:
      return 'text-foreground';
  }
}

export function RecentSubmissionOverview({
  entries,
  history,
  isSubmitting,
  visibleCount = DEFAULT_VISIBLE_COUNT,
  linkBuilder,
}: RecentSubmissionOverviewProps) {
  const { t } = useTranslation();
  const navigate = useNavigate();

  const rowsById = new Map<number, OverviewRow>();
  for (const item of history) {
    rowsById.set(item.id, {
      key: `history-${item.id}`,
      submissionId: item.id,
      pendingOrder: 0,
      status: item.status,
      verdict: (item.verdict as Verdict | null) ?? null,
      score: item.score ?? null,
      createdAt: item.created_at,
      timeUsed: item.time_used ?? null,
      memoryUsed: item.memory_used ?? null,
    });
  }

  const pendingRows: OverviewRow[] = [];
  for (const entry of entries) {
    if (!entry.submission) {
      pendingRows.push({
        key: `entry-${entry.id}`,
        submissionId: null,
        pendingOrder: entry.id,
        status: getEntryStatus(entry),
        verdict: null,
        score: null,
        createdAt: null,
        timeUsed: null,
        memoryUsed: null,
      });
      continue;
    }

    const sub = entry.submission;
    rowsById.set(sub.id, {
      key: `submission-${sub.id}`,
      submissionId: sub.id,
      pendingOrder: 0,
      status: sub.status,
      verdict: sub.result?.verdict ?? null,
      score: sub.result?.score ?? null,
      createdAt: sub.created_at,
      timeUsed: sub.result?.time_used ?? null,
      memoryUsed: sub.result?.memory_used ?? null,
    });
  }

  const mergedRows = [...pendingRows, ...Array.from(rowsById.values())]
    .sort((a, b) => {
      if (a.submissionId == null && b.submissionId == null) {
        return b.pendingOrder - a.pendingOrder;
      }
      if (a.submissionId == null) return -1;
      if (b.submissionId == null) return 1;
      const aTime = a.createdAt ? Date.parse(a.createdAt) : 0;
      const bTime = b.createdAt ? Date.parse(b.createdAt) : 0;
      return bTime - aTime;
    })
    .slice(0, visibleCount);

  if (mergedRows.length === 0) {
    return (
      <section className="rounded-lg border bg-card p-4">
        <h3 className="text-sm font-semibold">
          {t('result.latestOverviewTitle')}
        </h3>
        <p className="mt-2 text-sm text-muted-foreground">
          {isSubmitting ? t('result.judging') : t('result.latestOverviewEmpty')}
        </p>
      </section>
    );
  }

  return (
    <section className="rounded-lg border bg-card p-4">
      <h3 className="text-sm font-semibold">
        {t('result.latestOverviewTitle')}
      </h3>
      <div className="mt-3 space-y-2">
        {mergedRows.map((row) => {
          const { label, variant } = getVerdictBadge(
            row.verdict,
            row.status,
            t,
          );
          const scoreToneClass = getScoreToneClass(variant);
          const canOpen = row.submissionId != null && !!linkBuilder;

          return (
            <article
              key={row.key}
              className={`grid grid-cols-[auto_1fr] items-start gap-3 rounded-md border bg-muted/20 p-3 ${canOpen ? 'cursor-pointer transition-colors hover:bg-muted/30' : ''}`}
              onClick={
                canOpen
                  ? () => navigate(linkBuilder(row.submissionId as number))
                  : undefined
              }
              onKeyDown={
                canOpen
                  ? (event) => {
                      if (event.key === 'Enter' || event.key === ' ') {
                        event.preventDefault();
                        navigate(linkBuilder(row.submissionId as number));
                      }
                    }
                  : undefined
              }
              role={canOpen ? 'link' : undefined}
              tabIndex={canOpen ? 0 : undefined}
            >
              <div className="pt-0.5 font-mono text-xs text-muted-foreground tabular-nums">
                {row.submissionId != null ? `#${row.submissionId}` : '#...'}
              </div>

              <div>
                <div className="flex items-center justify-between gap-2">
                  <Badge variant={variant} className="text-xs">
                    {label}
                  </Badge>

                  {row.score != null && (
                    <div
                      className={`inline-flex items-end gap-1 font-mono tabular-nums ${scoreToneClass}`}
                    >
                      <span className="text-2xl font-extrabold leading-none tracking-tight">
                        {formatScore(row.score)}
                      </span>
                      <span className="pb-0.5 text-xs font-semibold tracking-wide opacity-85">
                        {t('result.pointsUnit')}
                      </span>
                    </div>
                  )}
                </div>

                <div className="mt-1 flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                  <span>
                    {row.createdAt
                      ? formatRelativeDatetime(row.createdAt, t)
                      : t('result.judging')}
                  </span>
                  {row.timeUsed != null && (
                    <span>
                      {t('result.time', { value: String(row.timeUsed) })}
                    </span>
                  )}
                  {row.memoryUsed != null && (
                    <span>
                      {t('result.memory', { value: String(row.memoryUsed) })}
                    </span>
                  )}
                </div>
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}
