import { useTranslation } from '@broccoli/sdk/i18n';
import { AlertCircle, CheckCircle2, Clock, XCircle } from 'lucide-react';

import { Badge } from '@/components/ui/badge';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';

type TestCaseStatus =
  | 'accepted'
  | 'wrong_answer'
  | 'time_limit'
  | 'runtime_error'
  | 'pending';

interface TestCase {
  id: number;
  status: TestCaseStatus;
  time?: number;
  memory?: number;
  message?: string;
}

interface SubmissionResultProps {
  status?: 'judging' | 'completed';
  verdict?: string;
  testCases?: TestCase[];
  totalTime?: number;
  totalMemory?: number;
}

const STATUS_CONFIG = {
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
  runtime_error: {
    icon: AlertCircle,
    labelKey: 'result.runtimeError',
    color: 'text-orange-500',
    bgColor: 'bg-orange-500/10',
  },
  pending: {
    icon: Clock,
    labelKey: 'result.pending',
    color: 'text-gray-500',
    bgColor: 'bg-gray-500/10',
  },
};

export function SubmissionResult({
  status,
  verdict,
  testCases,
  totalTime,
  totalMemory,
}: SubmissionResultProps) {
  const { t } = useTranslation();

  if (!status) {
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

  if (status === 'judging') {
    return (
      <Card className="h-full">
        <CardHeader>
          <CardTitle>{t('result.title')}</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-center h-32">
            <div className="flex flex-col items-center gap-2">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
              <p className="text-sm text-muted-foreground">
                {t('result.judging')}
              </p>
            </div>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card className="h-full">
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>{t('result.title')}</CardTitle>
          {verdict && (
            <Badge
              variant={verdict === 'Accepted' ? 'default' : 'destructive'}
              className="text-sm"
            >
              {verdict}
            </Badge>
          )}
        </div>
        {(totalTime !== undefined || totalMemory !== undefined) && (
          <div className="flex gap-4 text-sm text-muted-foreground mt-2">
            {totalTime !== undefined && (
              <div>{t('result.time', { value: String(totalTime) })}</div>
            )}
            {totalMemory !== undefined && (
              <div>{t('result.memory', { value: String(totalMemory) })}</div>
            )}
          </div>
        )}
      </CardHeader>
      <CardContent className="space-y-2">
        {testCases && testCases.length > 0 ? (
          testCases.map((testCase) => {
            const config = STATUS_CONFIG[testCase.status];
            const Icon = config.icon;

            return (
              <div
                key={testCase.id}
                className={`flex items-center justify-between p-3 rounded-lg border ${config.bgColor}`}
              >
                <div className="flex items-center gap-3">
                  <Icon className={`h-5 w-5 ${config.color}`} />
                  <div>
                    <div className="font-medium">
                      {t('result.testCase', { id: String(testCase.id) })}
                    </div>
                    {testCase.message && (
                      <div className="text-xs text-muted-foreground mt-1">
                        {testCase.message}
                      </div>
                    )}
                  </div>
                </div>
                <div className="text-right text-sm text-muted-foreground">
                  {testCase.time !== undefined && <div>{testCase.time}ms</div>}
                  {testCase.memory !== undefined && (
                    <div>{testCase.memory}MB</div>
                  )}
                </div>
              </div>
            );
          })
        ) : (
          <div className="text-center text-muted-foreground py-8">
            {t('result.noResults')}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
