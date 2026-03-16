import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { TestCaseResult, Verdict } from '@broccoli/web-sdk/submission';
import { cn } from '@broccoli/web-sdk/utils';
import {
  AlertCircle,
  CheckCircle2,
  Clock,
  MinusCircle,
  XCircle,
} from 'lucide-react';

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

export function getVerdictKey(verdict?: Verdict | null): VerdictKey {
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

export function formatMemory(kb: number): string {
  const mb = kb / 1024;
  return mb.toFixed(mb >= 10 ? 0 : 1);
}

export function TestCaseRow({
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
    <div className={cn('rounded-lg border', config.bgColor)}>
      <div className="flex items-center justify-between p-3">
        <div className="flex items-center gap-3">
          <Icon className={cn('h-5 w-5', config.color)} />
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
