import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Slot } from '@broccoli/web-sdk/slot';
import type {
  Submission,
  SubmissionSummary,
} from '@broccoli/web-sdk/submission';
import { Badge } from '@broccoli/web-sdk/ui';
import { formatRelativeDatetime } from '@broccoli/web-sdk/utils';
import { useQuery } from '@tanstack/react-query';
import { ChevronDown, ChevronRight, Inbox } from 'lucide-react';
import { useState } from 'react';

import { useAuth } from '@/features/auth/hooks/use-auth';
import { formatMemory } from '@/features/submission/components/TestCaseRow';
import type { SubmissionEntry } from '@/features/submission/hooks/use-submissions';
import { getVerdictBadge } from '@/features/submission/utils/verdict';

import { SubmissionResult } from '../../submission/components/SubmissionResult';
import { useProblemDockContext } from './dock/ProblemDockContext';

type UnifiedRow =
  | { kind: 'session'; entry: SubmissionEntry }
  | { kind: 'past'; summary: SubmissionSummary };

function getRowKey(row: UnifiedRow): string {
  return row.kind === 'session'
    ? `session-${row.entry.id}`
    : `past-${row.summary.id}`;
}

export function SubmissionsPanel() {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const { user } = useAuth();
  const { submissions, latestSubmission, contestId, problemId } =
    useProblemDockContext();
  const { entries, activeEntryId, setActiveEntryId } = submissions;
  const [expandedPastId, setExpandedPastId] = useState<number | null>(null);

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

  const sessionIds = new Set(
    entries.map((e) => e.submission?.id).filter(Boolean),
  );
  const rows: UnifiedRow[] = [
    ...entries.map((entry): UnifiedRow => ({ kind: 'session', entry })),
    ...(pastSubmissions ?? [])
      .filter((s) => !sessionIds.has(s.id))
      .map((summary): UnifiedRow => ({ kind: 'past', summary })),
  ];

  const isRowExpanded = (row: UnifiedRow): boolean => {
    if (row.kind === 'session') return activeEntryId === row.entry.id;
    return expandedPastId === row.summary.id;
  };

  const toggleRow = (row: UnifiedRow) => {
    if (row.kind === 'session') {
      setActiveEntryId(activeEntryId === row.entry.id ? null : row.entry.id);
    } else {
      setExpandedPastId(
        expandedPastId === row.summary.id ? null : row.summary.id,
      );
    }
  };

  if (rows.length === 0) {
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
        <table className="w-full text-[11px]">
          <thead className="sticky top-0 z-10">
            <tr className="border-b text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60 bg-muted/60 backdrop-blur-sm">
              <th className="w-5 px-3 py-1.5" />
              <th className="px-2 py-1.5 text-left font-medium">
                {t('overview.verdict')}
              </th>
              <th className="px-2 py-1.5 text-right font-medium">
                {t('overview.language')}
              </th>
              <th className="px-2 py-1.5 text-right font-medium">
                {t('result.timeHeader')}
              </th>
              <th className="px-2 py-1.5 text-right font-medium">
                {t('result.memoryHeader')}
              </th>
              <th className="px-2 py-1.5 text-right font-medium pr-3">
                {t('overview.submitted')}
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <SubmissionRow
                key={getRowKey(row)}
                row={row}
                isExpanded={isRowExpanded(row)}
                onToggle={() => toggleRow(row)}
              />
            ))}
          </tbody>
        </table>
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

function SubmissionRow({
  row,
  isExpanded,
  onToggle,
}: {
  row: UnifiedRow;
  isExpanded: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const isActive =
    row.kind === 'session' &&
    (row.entry.status === 'submitting' || row.entry.status === 'polling');

  let verdictNode: React.ReactNode = null;
  let language: string | null = null;
  let timeUsed: number | null | undefined = null;
  let memoryUsed: number | null | undefined = null;
  let createdAt: string | null = null;
  let submissionId: number | null = null;

  if (row.kind === 'session') {
    const { entry } = row;
    language = entry.submission?.language ?? null;
    timeUsed = entry.submission?.result?.time_used;
    memoryUsed = entry.submission?.result?.memory_used;
    createdAt = entry.submission?.created_at ?? null;
    submissionId = entry.submission?.id ?? null;
    verdictNode = <SessionVerdictCell entry={entry} />;
  } else {
    const { summary } = row;
    language = summary.language;
    timeUsed = summary.time_used;
    memoryUsed = summary.memory_used;
    createdAt = summary.created_at;
    submissionId = summary.id;
    const { label, variant } = getVerdictBadge(
      summary.verdict ?? null,
      summary.status,
      t,
    );
    verdictNode = label ? (
      <Badge variant={variant} className="text-[10px] px-1.5 py-0 h-4">
        {label}
      </Badge>
    ) : null;
  }

  return (
    <>
      <tr
        onClick={onToggle}
        className={`border-b border-border/40 last:border-b-0 cursor-pointer transition-colors duration-75 hover:bg-muted/40 ${
          isExpanded ? 'bg-muted/30' : ''
        } ${isActive ? 'bg-primary/[0.03]' : ''}`}
      >
        <td className="px-3 py-2 text-muted-foreground/60">
          {isExpanded ? (
            <ChevronDown className="h-3.5 w-3.5" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5" />
          )}
        </td>
        <td className="px-2 py-2">
          <span className="flex items-center gap-1.5">
            {verdictNode}
            {!!submissionId && (
              <span className="text-[10px] font-mono text-muted-foreground/40">
                #{submissionId}
              </span>
            )}
          </span>
        </td>
        <td className="px-2 py-2 font-mono text-muted-foreground/60 text-right whitespace-nowrap">
          {language ?? '—'}
        </td>
        <td className="px-2 py-2 font-mono text-muted-foreground/60 text-right whitespace-nowrap tabular-nums">
          {timeUsed != null ? `${timeUsed}ms` : '—'}
        </td>
        <td className="px-2 py-2 font-mono text-muted-foreground/60 text-right whitespace-nowrap tabular-nums">
          {memoryUsed != null ? `${formatMemory(memoryUsed)}MB` : '—'}
        </td>
        <td className="px-2 py-2 text-muted-foreground/50 text-right whitespace-nowrap pr-3">
          {createdAt ? formatRelativeDatetime(createdAt, t) : '—'}
        </td>
      </tr>
      {isExpanded && (
        <tr>
          <td
            colSpan={6}
            className="px-3 pb-3 pt-0.5 border-b border-border/40"
          >
            {row.kind === 'session' ? (
              <SubmissionResult
                submission={row.entry.submission}
                isSubmitting={row.entry.status === 'submitting'}
                error={row.entry.error}
              />
            ) : (
              <PastSubmissionDetail submissionId={row.summary.id} />
            )}
          </td>
        </tr>
      )}
    </>
  );
}

/** Fetches full submission detail on demand when a past submission row is expanded */
function PastSubmissionDetail({ submissionId }: { submissionId: number }) {
  const apiClient = useApiClient();

  const { data: submission, isLoading } = useQuery<Submission>({
    queryKey: ['submission', submissionId],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/submissions/{id}', {
        params: { path: { id: submissionId } },
      });
      if (error) throw error;
      return data;
    },
    staleTime: 60_000,
  });

  if (isLoading || !submission) {
    return (
      <div className="flex items-center justify-center py-4">
        <span className="h-4 w-4 rounded-full border-2 border-primary border-t-transparent animate-spin" />
      </div>
    );
  }

  return <SubmissionResult submission={submission} />;
}

function SessionVerdictCell({ entry }: { entry: SubmissionEntry }) {
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

  if (entry.submission) {
    const { label, variant } = getVerdictBadge(
      entry.submission.result?.verdict ?? null,
      entry.submission.status,
      t,
    );
    if (label) {
      return (
        <Badge variant={variant} className="text-[10px] px-1.5 py-0 h-4">
          {label}
        </Badge>
      );
    }
  }

  return null;
}
