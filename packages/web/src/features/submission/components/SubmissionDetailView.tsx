import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Badge } from '@broccoli/web-sdk/ui';
import { formatRelativeDatetime } from '@broccoli/web-sdk/utils';
import { AlertCircle, ArrowLeft, Loader2 } from 'lucide-react';
import { Link } from 'react-router';

import {
  TERMINAL_STATUSES,
  useSubmissionDetail,
} from '@/features/submission/hooks/use-submission-detail';
import { getVerdictBadge } from '@/features/submission/utils/verdict';

import { SubmissionResult } from './SubmissionResult';
import { formatMemory } from './TestCaseRow';

interface SubmissionDetailViewProps {
  submissionId: number;
  /** If provided, links point to contest-scoped routes. Otherwise standalone. */
  contestId?: number;
}

export function SubmissionDetailView({
  submissionId,
  contestId,
}: SubmissionDetailViewProps) {
  const { t } = useTranslation();
  const { submission, isLoading, error } = useSubmissionDetail(submissionId);

  const backTo = contestId ? `/contests/${contestId}/submissions` : '/';

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-20">
        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error || !submission) {
    return (
      <div className="space-y-4">
        <BackLink to={backTo} />
        <div className="flex flex-col items-center gap-3 py-20">
          <AlertCircle className="h-8 w-8 text-muted-foreground" />
          <p className="text-sm text-muted-foreground">
            {t('submissionDetail.notFound')}
          </p>
        </div>
      </div>
    );
  }

  const status = submission.status;
  const result = submission.result;
  const isTerminal = TERMINAL_STATUSES.has(status);

  const { label: verdictLabel, variant: verdictVariant } = getVerdictBadge(
    result?.verdict ?? null,
    status,
    t,
  );

  const problemLink = contestId
    ? `/contests/${contestId}/problems/${submission.problem_id}`
    : `/problems/${submission.problem_id}`;

  return (
    <div className="max-w-4xl mx-auto space-y-6">
      <BackLink to={backTo} />

      {/* Header card */}
      <div className="rounded-lg border bg-card">
        <div className="px-6 py-5">
          <div className="flex flex-wrap items-center gap-3">
            <h1 className="text-xl font-bold tabular-nums">#{submission.id}</h1>
            <Badge variant={verdictVariant} className="text-sm">
              {!isTerminal && <Loader2 className="mr-1 h-3 w-3 animate-spin" />}
              {verdictLabel}
            </Badge>
            {result?.score != null && (
              <span className="ml-auto font-mono text-lg font-bold tabular-nums text-foreground">
                {result.score}
                <span className="text-sm font-normal text-muted-foreground ml-1">
                  pts
                </span>
              </span>
            )}
          </div>

          <div className="mt-4 grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-y-3 gap-x-6 text-sm">
            <MetaItem label={t('submissionDetail.problem')}>
              <Link
                to={problemLink}
                className="font-medium text-foreground hover:text-primary hover:underline transition-colors"
              >
                {submission.problem_title}
              </Link>
            </MetaItem>
            <MetaItem label={t('submissionDetail.user')}>
              <span className="font-medium text-foreground">
                {submission.username}
              </span>
            </MetaItem>
            <MetaItem label={t('submissionDetail.language')}>
              <Badge variant="outline" className="font-mono">
                {submission.language}
              </Badge>
            </MetaItem>
            <MetaItem label={t('overview.submitted')}>
              <span className="text-foreground">
                {formatRelativeDatetime(submission.created_at, t)}
              </span>
            </MetaItem>
            {result?.time_used != null && (
              <MetaItem label={t('result.timeHeader')}>
                <span className="font-mono tabular-nums text-foreground">
                  {result.time_used}ms
                </span>
              </MetaItem>
            )}
            {result?.memory_used != null && (
              <MetaItem label={t('result.memoryHeader')}>
                <span className="font-mono tabular-nums text-foreground">
                  {formatMemory(result.memory_used)}MB
                </span>
              </MetaItem>
            )}
          </div>
        </div>
      </div>

      {/* Submission content */}
      <div className="rounded-lg border bg-card px-6 py-5">
        {!isTerminal && (
          <div className="flex items-center gap-2 pb-4">
            <Loader2 className="h-4 w-4 animate-spin text-primary" />
            <span className="text-sm text-muted-foreground">
              {status === 'Compiling'
                ? t('result.compiling')
                : status === 'Running'
                  ? t('result.running')
                  : t('result.judging')}
            </span>
          </div>
        )}

        <SubmissionResult submission={submission} />
      </div>
    </div>
  );
}

function MetaItem({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-0.5">
      <div className="text-[11px] font-medium uppercase tracking-wider text-muted-foreground/60">
        {label}
      </div>
      <div>{children}</div>
    </div>
  );
}

function BackLink({ to }: { to: string }) {
  const { t } = useTranslation();
  return (
    <Link
      to={to}
      className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors"
    >
      <ArrowLeft className="h-4 w-4" />
      {t('submissionDetail.backToList')}
    </Link>
  );
}
