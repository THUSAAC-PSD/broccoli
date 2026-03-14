import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Slot } from '@broccoli/web-sdk/slot';
import type {
  Submission,
  TestCaseResult,
  Verdict,
} from '@broccoli/web-sdk/submission';
import {
  Badge,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
} from '@broccoli/web-sdk/ui';
import {
  AlertCircle,
  CheckCircle2,
  Clock,
  MinusCircle,
  Timer,
  XCircle,
} from 'lucide-react';

import type { SubmissionError } from '@/features/submission/hooks/use-submission';
import { getVerdictBadge } from '@/features/submission/utils/verdict';

type VerdictKey =
  | 'accepted'
  | 'wrong_answer'
  | 'time_limit'
  | 'memory_limit'
  | 'runtime_error'
  | 'system_error'
  | 'skipped'
  | 'custom'
  | 'pending';

const VERDICT_CONFIG: Record<
  VerdictKey,
  {
    icon: typeof CheckCircle2;
    color: string;
    bgColor: string;
  }
> = {
  accepted: {
    icon: CheckCircle2,
    color: 'text-green-500',
    bgColor: 'bg-green-500/10',
  },
  wrong_answer: {
    icon: XCircle,
    color: 'text-red-500',
    bgColor: 'bg-red-500/10',
  },
  time_limit: {
    icon: Clock,
    color: 'text-yellow-500',
    bgColor: 'bg-yellow-500/10',
  },
  memory_limit: {
    icon: Clock,
    color: 'text-yellow-500',
    bgColor: 'bg-yellow-500/10',
  },
  runtime_error: {
    icon: AlertCircle,
    color: 'text-orange-500',
    bgColor: 'bg-orange-500/10',
  },
  system_error: {
    icon: AlertCircle,
    color: 'text-gray-500',
    bgColor: 'bg-gray-500/10',
  },
  skipped: {
    icon: MinusCircle,
    color: 'text-gray-400',
    bgColor: 'bg-gray-400/10',
  },
  custom: {
    icon: AlertCircle,
    color: 'text-blue-500',
    bgColor: 'bg-blue-500/10',
  },
  pending: {
    icon: Clock,
    color: 'text-gray-500',
    bgColor: 'bg-gray-500/10',
  },
};

function getVerdictKey(verdict?: Verdict | null): VerdictKey {
  switch (verdict) {
    case 'Accepted':
      return 'accepted';
    case 'WrongAnswer':
      return 'wrong_answer';
    case 'TimeLimitExceeded':
      return 'time_limit';
    case 'MemoryLimitExceeded':
      return 'memory_limit';
    case 'RuntimeError':
      return 'runtime_error';
    case 'SystemError':
      return 'system_error';
    case 'Skipped':
      return 'skipped';
    case null:
    case undefined:
      return 'pending';
    default:
      return 'custom';
  }
}

function formatMemory(kb: number): string {
  const mb = kb / 1024;
  return mb.toFixed(mb >= 10 ? 0 : 1);
}

interface SubmissionResultProps {
  submission?: Submission | null;
  isSubmitting?: boolean;
  error?: SubmissionError | null;
}

export function SubmissionResult({
  submission,
  isSubmitting,
  error,
}: SubmissionResultProps) {
  const { t } = useTranslation();

  // No submission yet — prompt
  if (!submission && !isSubmitting && !error) {
    return (
      <Card className="h-full">
        <CardHeader>
          <CardTitle>{t('result.title')}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-center h-32 text-muted-foreground">
            {t('result.submitPrompt')}
          </div>
        </CardContent>
      </Card>
    );
  }

  // Submission error
  if (error && !submission) {
    return (
      <Slot name="submission-result.rejection" slotProps={{ error }}>
        <GenericRejectionCard error={error} />
      </Slot>
    );
  }

  // Pure spinner states: Pending, Compiling (no data to show yet)
  const status = submission?.status;
  if (
    (!submission && isSubmitting) ||
    status === 'Pending' ||
    status === 'Compiling'
  ) {
    const statusLabel =
      status === 'Compiling' ? t('result.compiling') : t('result.judging');

    return (
      <Card className="h-full">
        <CardHeader>
          <CardTitle>{t('result.title')}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-center h-32">
            <div className="flex flex-col items-center gap-2">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
              <p className="text-sm text-muted-foreground">{statusLabel}</p>
            </div>
          </div>
        </CardContent>
      </Card>
    );
  }

  if (!submission) return null;

  const result = submission.result;
  const isRunning = status === 'Running';
  const testCases = result?.test_case_results ?? [];

  if (isRunning) {
    return (
      <Card className="h-full">
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>{t('result.title')}</CardTitle>
            <div className="flex items-center gap-2">
              <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-primary"></div>
              <span className="text-sm text-muted-foreground">
                {t('result.running')}
              </span>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-2">
          <Slot
            name="submission-result.content"
            as="div"
            className="space-y-2"
            slotProps={{ submission, testCases }}
          >
            {testCases.map((tc, index) => (
              <TestCaseRow key={tc.id} testCase={tc} index={index + 1} />
            ))}
          </Slot>
        </CardContent>
      </Card>
    );
  }

  // Terminal states
  const { label: verdictLabel, variant: verdictVariant } = getVerdictBadge(
    submission.result?.verdict ?? null,
    submission.status,
    t,
  );
  const totalTime = result?.time_used;
  const totalMemory = result?.memory_used;

  return (
    <Card className="h-full">
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>{t('result.title')}</CardTitle>
          {verdictLabel && (
            <Badge variant={verdictVariant} className="text-sm">
              {verdictLabel}
            </Badge>
          )}
        </div>
        {(totalTime != null || totalMemory != null) && (
          <div className="flex gap-4 text-sm text-muted-foreground mt-2">
            {totalTime != null && (
              <div>{t('result.time', { value: String(totalTime) })}</div>
            )}
            {totalMemory != null && (
              <div>
                {t('result.memory', { value: formatMemory(totalMemory) })}
              </div>
            )}
          </div>
        )}
      </CardHeader>
      <CardContent className="space-y-2">
        {/* Compilation error output */}
        {status === 'CompilationError' && result?.compile_output && (
          <div className="mb-4">
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
          <div className="mb-4 text-sm text-destructive space-y-1">
            <div className="font-medium">{t('result.systemMessage')}</div>
            <pre className="text-xs bg-muted p-3 rounded-lg overflow-x-auto whitespace-pre-wrap">
              {result.error_message}
            </pre>
          </div>
        )}

        {/* Test case results */}
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
      </CardContent>
    </Card>
  );
}
function GenericRejectionCard({ error }: { error: SubmissionError }) {
  const { t } = useTranslation();

  if (error.code === 'RATE_LIMITED') {
    return (
      <Card className="h-full">
        <CardHeader>
          <CardTitle>{t('result.title')}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex flex-col items-center gap-3 py-6">
            <div className="rounded-full bg-amber-500/10 p-3">
              <Timer className="h-6 w-6 text-amber-500" />
            </div>
            <div className="text-center space-y-1">
              <p className="text-sm font-medium">
                {t('result.rejection.rateLimited')}
              </p>
              <p className="text-xs text-muted-foreground">{error.message}</p>
            </div>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className="h-full">
      <CardHeader>
        <CardTitle>{t('result.title')}</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="flex flex-col items-center gap-3 py-6">
          <div className="rounded-full bg-destructive/10 p-3">
            <XCircle className="h-6 w-6 text-destructive" />
          </div>
          <div className="text-center space-y-1">
            <p className="text-sm font-medium">
              {t('result.rejection.failed')}
            </p>
            <p className="text-xs text-muted-foreground">{error.message}</p>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function TestCaseRow({
  testCase,
  index,
}: {
  testCase: TestCaseResult;
  index: number;
}) {
  const { t } = useTranslation();
  const verdictKey = getVerdictKey(testCase.verdict);
  const config = VERDICT_CONFIG[verdictKey];
  const Icon = config.icon;

  return (
    <div className={`rounded-lg border ${config.bgColor}`}>
      <div className="flex items-center justify-between p-3">
        <div className="flex items-center gap-3">
          <Icon className={`h-5 w-5 ${config.color}`} />
          <div>
            <div className="font-medium">
              {t('result.testCase', { id: String(index) })}
            </div>
            {testCase.checker_output && (
              <div className="text-xs text-muted-foreground mt-1">
                {t('result.checkerOutput')}: {testCase.checker_output}
              </div>
            )}
          </div>
        </div>
        <div className="text-right text-sm text-muted-foreground">
          {testCase.time_used != null && (
            <div>
              {t('result.timeValue', { value: String(testCase.time_used) })}
            </div>
          )}
          {testCase.memory_used != null && (
            <div>
              {t('result.memoryValue', {
                value: formatMemory(testCase.memory_used),
              })}
            </div>
          )}
        </div>
      </div>
      {(testCase.input ||
        testCase.expected_output ||
        testCase.stdout ||
        testCase.stderr) && (
        <div className="px-3 pb-3 space-y-2">
          {testCase.input && (
            <div>
              <div className="text-xs font-medium text-muted-foreground mb-1">
                {t('result.input')}
              </div>
              <pre className="text-xs bg-muted p-2 rounded overflow-x-auto whitespace-pre-wrap">
                {testCase.input}
              </pre>
            </div>
          )}
          {testCase.expected_output && (
            <div>
              <div className="text-xs font-medium text-muted-foreground mb-1">
                {t('result.expectedOutput')}
              </div>
              <pre className="text-xs bg-muted p-2 rounded overflow-x-auto whitespace-pre-wrap">
                {testCase.expected_output}
              </pre>
            </div>
          )}
          {testCase.stdout && (
            <div>
              <div className="text-xs font-medium text-muted-foreground mb-1">
                {t('result.stdout')}
              </div>
              <pre className="text-xs bg-muted p-2 rounded overflow-x-auto whitespace-pre-wrap">
                {testCase.stdout}
              </pre>
            </div>
          )}
          {testCase.stderr && (
            <div>
              <div className="text-xs font-medium text-muted-foreground mb-1">
                {t('result.stderr')}
              </div>
              <pre className="text-xs bg-muted p-2 rounded overflow-x-auto whitespace-pre-wrap">
                {testCase.stderr}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
