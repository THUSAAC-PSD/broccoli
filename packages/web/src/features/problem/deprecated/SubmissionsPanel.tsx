import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Slot } from '@broccoli/web-sdk/slot';
import type { SubmissionSummary } from '@broccoli/web-sdk/submission';
import { Badge } from '@broccoli/web-sdk/ui';
import { useQuery } from '@tanstack/react-query';
import { Inbox } from 'lucide-react';

import { useAuth } from '@/features/auth/hooks/use-auth';
import { SubmissionResult } from '@/features/submission/components/SubmissionResult';
import { SubmissionsTable } from '@/features/submission/components/SubmissionsTable';
import type { SubmissionEntry } from '@/features/submission/hooks/use-submissions';

import { useProblemDockContext } from './dock/ProblemDockContext';

/**
 * Synthetic ID for session entries that don't have a server submission yet.
 * Negative to avoid collision with real submission IDs.
 */
function syntheticId(entryId: number): number {
  return -entryId;
}

export function SubmissionsPanel() {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const { user } = useAuth();
  const { submissions, latestSubmission, contestId, problemId } =
    useProblemDockContext();
  const { entries } = submissions;

  // Fetch past submissions from API
  const { data: pastSubmissions } = useQuery({
    queryKey: [
      'panel-past-submissions',
      contestId,
      problemId,
      user?.id,
      entries.filter((e) => e.status === 'done').length,
    ],
    enabled: !!user,
    queryFn: async () => {
      if (!user) return [];
      if (contestId) {
        const { data, error } = await apiClient.GET(
          '/contests/{id}/submissions',
          {
            params: {
              path: { id: contestId },
              query: {
                problem_id: problemId,
                user_id: user.id,
                per_page: 50,
                sort_by: 'created_at',
                sort_order: 'desc',
              },
            },
          },
        );
        if (error) return [];
        return data.data;
      }
      return [];
    },
    staleTime: 30_000,
  });

  // Convert ALL session entries to summaries (including pending/error with synthetic IDs)
  const sessionSummaries: SubmissionSummary[] = entries.map((e) =>
    entryToSummary(e),
  );
  const realSessionIds = new Set(
    entries
      .map((e) => e.submission?.id)
      .filter((id): id is number => id != null),
  );
  const allSubmissions: SubmissionSummary[] = [
    ...sessionSummaries,
    ...(pastSubmissions ?? []).filter((s) => !realSessionIds.has(s.id)),
  ];

  const entryBySummaryId = new Map<number, SubmissionEntry>();
  for (const entry of entries) {
    const id = entry.submission?.id ?? syntheticId(entry.id);
    entryBySummaryId.set(id, entry);
  }

  // Auto-expand the latest entry
  const latestEntry = entries[0];
  const autoExpandId = latestEntry
    ? (latestEntry.submission?.id ?? syntheticId(latestEntry.id))
    : null;

  if (allSubmissions.length === 0) {
    return (
      <div className="h-full flex flex-col">
        <div className="flex-1 flex flex-col items-center justify-center text-muted-foreground gap-3 p-6">
          <div className="rounded-full bg-muted p-3">
            <Inbox className="h-5 w-5 opacity-40" />
          </div>
          <div className="text-center space-y-0.5">
            <p className="text-sm font-medium text-foreground/50">
              {t('result.noSubmissions')}
            </p>
            <p className="text-xs">{t('result.submitPrompt')}</p>
          </div>
        </div>
        <Slot
          name="problem-detail.sidebar"
          as="div"
          slotProps={{
            submission: latestSubmission,
            submissions: entries,
            contestId,
            problemId,
          }}
        />
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto">
        <SubmissionsTable
          submissions={allSubmissions}
          columns={SubmissionsTable.compactColumns}
          compact
          stickyHeader
          autoExpandId={autoExpandId}
          renderVerdict={(sub) => {
            const entry = entryBySummaryId.get(sub.id);
            if (!entry) return undefined;

            // Only override for in-progress/error states; done entries fall through to default badge
            if (entry.status === 'done') return undefined;
            return <SessionVerdictBadge entry={entry} />;
          }}
          renderExpandedDetail={(sub) => {
            const entry = entryBySummaryId.get(sub.id);
            if (!entry) return undefined; // use default API fetch

            // Session entries: render inline result or error
            if (entry.status === 'error') {
              return <SubmissionResult error={entry.error} />;
            }
            return (
              <SubmissionResult
                submission={entry.submission}
                isSubmitting={entry.status === 'submitting'}
              />
            );
          }}
          rowClassName={(sub) => {
            const entry = entryBySummaryId.get(sub.id);
            if (
              entry &&
              (entry.status === 'submitting' || entry.status === 'polling')
            ) {
              return 'bg-primary/[0.03]';
            }
            if (entry?.status === 'error') {
              return 'bg-destructive/[0.03]';
            }
            return '';
          }}
        />
      </div>

      <Slot
        name="problem-detail.sidebar"
        as="div"
        className="flex-shrink-0"
        slotProps={{
          submission: latestSubmission,
          submissions: entries,
          contestId,
          problemId,
        }}
      />
    </div>
  );
}

/** Convert a session entry to SubmissionSummary (synthetic for entries without server submission). */
function entryToSummary(entry: SubmissionEntry): SubmissionSummary {
  if (entry.submission) {
    const sub = entry.submission;
    return {
      id: sub.id,
      language: sub.language,
      status: sub.status,
      verdict: sub.result?.verdict ?? null,
      user_id: 0,
      username: '',
      problem_id: sub.problem_id,
      problem_title: sub.problem_title ?? '',
      contest_id: sub.contest_id ?? null,
      contest_type: sub.contest_type ?? '',
      created_at: sub.created_at,
      score: sub.result?.score ?? null,
      time_used: sub.result?.time_used ?? null,
      memory_used: sub.result?.memory_used ?? null,
    };
  }

  // No server submission yet (submitting or error) — synthetic placeholder
  return {
    id: syntheticId(entry.id),
    language: '',
    status: 'Pending',
    verdict: null,
    user_id: 0,
    username: '',
    problem_id: 0,
    problem_title: '',
    contest_id: null,
    contest_type: '',
    created_at: new Date().toISOString(),
    score: null,
    time_used: null,
    memory_used: null,
  };
}

function SessionVerdictBadge({ entry }: { entry: SubmissionEntry }) {
  const { t } = useTranslation();

  if (entry.status === 'submitting') {
    return (
      <span className="flex items-center gap-1.5">
        <span className="h-3 w-3 rounded-full border-2 border-primary border-t-transparent animate-spin" />
        <span className="text-[11px] text-muted-foreground">
          {t('result.submitting')}
        </span>
      </span>
    );
  }

  if (entry.status === 'polling') {
    const label =
      entry.submission?.status === 'Compiling'
        ? t('result.compilingShort')
        : entry.submission?.status === 'Running'
          ? t('result.runningShort')
          : t('result.judging');
    return (
      <span className="flex items-center gap-1.5">
        <span className="h-3 w-3 rounded-full border-2 border-primary border-t-transparent animate-spin" />
        <span className="text-[11px] text-muted-foreground">{label}</span>
      </span>
    );
  }

  if (entry.status === 'error') {
    return (
      <Badge variant="destructive" className="text-[10px] px-1.5 py-0 h-4">
        {t('result.rejection.failed')}
      </Badge>
    );
  }

  // Fallback to default rendering
  return undefined;
}
