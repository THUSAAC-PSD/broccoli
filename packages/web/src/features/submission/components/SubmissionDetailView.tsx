import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Slot } from '@broccoli/web-sdk/slot';
import {
  Badge,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from '@broccoli/web-sdk/ui';
import { formatRelativeDatetime } from '@broccoli/web-sdk/utils';
import { AlertCircle, ArrowLeft, Loader2 } from 'lucide-react';
import { Link } from 'react-router';

import {
  TERMINAL_STATUSES,
  useSubmissionDetail,
} from '@/features/submission/hooks/use-submission-detail';
import { getVerdictBadge } from '@/features/submission/utils/verdict';

import { ReadOnlyCodeViewer } from './ReadOnlyCodeViewer';
import { formatMemory, TestCaseRow } from './TestCaseRow';

interface SubmissionDetailViewProps {
  submissionId: number;
  contestId: number;
}

export function SubmissionDetailView({
  submissionId,
  contestId,
}: SubmissionDetailViewProps) {
  const { t } = useTranslation();
  const { submission, isLoading, error } = useSubmissionDetail(submissionId);

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
        <BackLink contestId={contestId} />
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
  const testCases = result?.test_case_results ?? [];
  const isTerminal = TERMINAL_STATUSES.has(status);
  const isRunning = status === 'Running';

  const { label: verdictLabel, variant: verdictVariant } = getVerdictBadge(
    result?.verdict ?? null,
    status,
    t,
  );

  return (
    <div className="space-y-4">
      <BackLink contestId={contestId} />

      <Card>
        {/* Metadata header */}
        <CardHeader>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <CardTitle className="text-lg">#{submission.id}</CardTitle>
              <Badge variant={verdictVariant} className="text-sm">
                {!isTerminal && (
                  <Loader2 className="mr-1 h-3 w-3 animate-spin" />
                )}
                {verdictLabel}
              </Badge>
            </div>

            {result?.score != null && (
              <span className="font-mono text-lg font-bold tabular-nums text-foreground">
                {t('submissionDetail.score')}: {result.score}
              </span>
            )}
          </div>

          {/* Meta row */}
          <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-sm text-muted-foreground">
            <span>
              {t('submissionDetail.problem')}:{' '}
              <Link
                to={`/contests/${contestId}/problems/${submission.problem_id}`}
                className="font-medium text-foreground hover:text-primary hover:underline"
              >
                {submission.problem_title}
              </Link>
            </span>
            <span>
              {t('submissionDetail.user')}: {submission.username}
            </span>
            <span>
              {t('submissionDetail.language')}:{' '}
              <Badge variant="outline" className="ml-0.5">
                {submission.language}
              </Badge>
            </span>
            <span>
              {t('submissionDetail.submittedAt', {
                value: formatRelativeDatetime(submission.created_at, t),
              })}
            </span>
            {result?.time_used != null && (
              <span>
                {t('result.time', { value: String(result.time_used) })}
              </span>
            )}
            {result?.memory_used != null && (
              <span>
                {t('result.memory', {
                  value: formatMemory(result.memory_used),
                })}
              </span>
            )}
          </div>
        </CardHeader>

        <CardContent className="space-y-4">
          {/* Code viewer — expanded by default on detail page */}
          {submission.files && submission.files.length > 0 && (
            <ReadOnlyCodeViewer
              files={submission.files}
              language={submission.language}
              defaultOpen
            />
          )}

          {/* Compilation error output */}
          {status === 'CompilationError' && result?.compile_output && (
            <div>
              <div className="text-sm font-medium mb-1">
                {t('result.compileOutput')}
              </div>
              <pre className="text-xs bg-muted p-3 rounded-lg overflow-x-auto whitespace-pre-wrap">
                {result.compile_output}
              </pre>
            </div>
          )}

          {/* System error message */}
          {status === 'SystemError' && result?.error_message && (
            <div className="text-sm text-destructive space-y-1">
              <div className="font-medium">{t('result.systemMessage')}</div>
              <pre className="text-xs bg-muted p-3 rounded-lg overflow-x-auto whitespace-pre-wrap">
                {result.error_message}
              </pre>
            </div>
          )}

          {/* Running indicator */}
          {!isTerminal && (
            <div className="flex items-center gap-2 py-2">
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

          {/* Test case results via slot */}
          {(isRunning || isTerminal) && (
            <Slot
              name="submission-result.content"
              as="div"
              className="space-y-2"
              slotProps={{ submission, testCases }}
            >
              {testCases.length > 0
                ? testCases.map((tc, index) => (
                    <TestCaseRow key={tc.id} testCase={tc} index={index + 1} />
                  ))
                : status === 'Judged' && (
                    <div className="text-center text-muted-foreground py-8">
                      {t('result.noResults')}
                    </div>
                  )}
            </Slot>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function BackLink({ contestId }: { contestId: number }) {
  const { t } = useTranslation();
  return (
    <Link
      to={`/contests/${contestId}/submissions`}
      className="inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground transition-colors"
    >
      <ArrowLeft className="h-4 w-4" />
      {t('submissionDetail.backToList')}
    </Link>
  );
}
