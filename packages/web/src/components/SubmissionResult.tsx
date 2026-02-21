import type {
  SubmissionResponse,
  TestCaseResultResponse,
  Verdict,
} from '@broccoli/sdk';
import { useTranslation } from '@broccoli/sdk/i18n';
import { AlertCircle, CheckCircle2, Clock, XCircle } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';

type VerdictKey =
  | 'accepted'
  | 'wrong_answer'
  | 'time_limit'
  | 'memory_limit'
  | 'runtime_error'
  | 'system_error'
  | 'pending';

const VERDICT_CONFIG: Record<
  VerdictKey,
  {
    icon: typeof CheckCircle2;
    labelKey: string;
    color: string;
    bgColor: string;
  }
> = {
  accepted: {
    icon: CheckCircle2,
    labelKey: 'result.accepted',
    color: 'text-green-500',
    bgColor: 'bg-green-500/10',
  },
  wrong_answer: {
    icon: XCircle,
    labelKey: 'result.wrongAnswer',
    color: 'text-red-500',
    bgColor: 'bg-red-500/10',
  },
  time_limit: {
    icon: Clock,
    labelKey: 'result.timeLimit',
    color: 'text-yellow-500',
    bgColor: 'bg-yellow-500/10',
  },
  memory_limit: {
    icon: Clock,
    labelKey: 'result.memoryLimit',
    color: 'text-yellow-500',
    bgColor: 'bg-yellow-500/10',
  },
  runtime_error: {
    icon: AlertCircle,
    labelKey: 'result.runtimeError',
    color: 'text-orange-500',
    bgColor: 'bg-orange-500/10',
  },
  system_error: {
    icon: AlertCircle,
    labelKey: 'result.systemError',
    color: 'text-gray-500',
    bgColor: 'bg-gray-500/10',
  },
  pending: {
    icon: Clock,
    labelKey: 'result.pending',
    color: 'text-gray-500',
    bgColor: 'bg-gray-500/10',
  },
};

const VERDICT_MAP: Record<Verdict, VerdictKey> = {
  Accepted: 'accepted',
  WrongAnswer: 'wrong_answer',
  TimeLimitExceeded: 'time_limit',
  MemoryLimitExceeded: 'memory_limit',
  RuntimeError: 'runtime_error',
  SystemError: 'system_error',
};

function formatMemory(kb: number): string {
  const mb = kb / 1024;
  return mb.toFixed(mb >= 10 ? 0 : 1);
}

interface SubmissionResultProps {
  submission?: SubmissionResponse | null;
  isSubmitting?: boolean;
  error?: string | null;
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
      <Card className="h-full">
        <CardHeader>
          <CardTitle>{t('result.title')}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-center h-32 text-destructive">
            {t('result.submitError')}
          </div>
        </CardContent>
      </Card>
    );
  }

  // In-progress states: Pending, Compiling, Running
  const status = submission?.status;
  if (
    isSubmitting ||
    status === 'Pending' ||
    status === 'Compiling' ||
    status === 'Running'
  ) {
    const statusLabel =
      status === 'Compiling'
        ? t('result.compiling')
        : status === 'Running'
          ? t('result.running')
          : t('result.judging');

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

  // Terminal states
  if (!submission) return null;

  const result = submission.result;
  const verdictLabel = getVerdictLabel(submission, t);
  const verdictVariant = getVerdictBadgeVariant(submission);

  const totalTime = result?.time_used;
  const totalMemory = result?.memory_used;
  const testCases = result?.test_case_results ?? [];

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
          <div className="mb-4 text-sm text-destructive">
            {result.error_message}
          </div>
        )}

        {/* Test case results */}
        {testCases.length > 0
          ? testCases.map((tc, index) => (
              <TestCaseRow key={tc.id} testCase={tc} index={index + 1} />
            ))
          : status === 'Judged' && (
              <div className="text-center text-muted-foreground py-8">
                {t('result.noResults')}
              </div>
            )}
      </CardContent>
    </Card>
  );
}

function TestCaseRow({
  testCase,
  index,
}: {
  testCase: TestCaseResultResponse;
  index: number;
}) {
  const { t } = useTranslation();
  const verdictKey = VERDICT_MAP[testCase.verdict] ?? 'pending';
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
                {testCase.checker_output}
              </div>
            )}
          </div>
        </div>
        <div className="text-right text-sm text-muted-foreground">
          {testCase.time_used != null && <div>{testCase.time_used}ms</div>}
          {testCase.memory_used != null && (
            <div>{formatMemory(testCase.memory_used)}MB</div>
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

function getVerdictLabel(
  submission: SubmissionResponse,
  t: (key: string) => string,
): string | null {
  if (submission.status === 'CompilationError') {
    return t('result.compilationError');
  }
  if (submission.status === 'SystemError') {
    return t('result.systemError');
  }
  const verdict = submission.result?.verdict;
  if (!verdict) return null;
  const key = VERDICT_MAP[verdict];
  return key ? t(VERDICT_CONFIG[key].labelKey) : verdict;
}

function getVerdictBadgeVariant(
  submission: SubmissionResponse,
): 'default' | 'destructive' | 'secondary' {
  if (submission.status === 'CompilationError') return 'secondary';
  if (submission.status === 'SystemError') return 'secondary';
  const verdict = submission.result?.verdict;
  if (verdict === 'Accepted') return 'default';
  return 'destructive';
}
