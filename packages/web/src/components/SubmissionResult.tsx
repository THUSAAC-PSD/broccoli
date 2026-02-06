import { AlertCircle,CheckCircle2, Clock, XCircle } from 'lucide-react';

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
    label: 'Accepted',
    color: 'text-green-500',
    bgColor: 'bg-green-500/10',
  },
  wrong_answer: {
    icon: XCircle,
    label: 'Wrong Answer',
    color: 'text-red-500',
    bgColor: 'bg-red-500/10',
  },
  time_limit: {
    icon: Clock,
    label: 'Time Limit',
    color: 'text-yellow-500',
    bgColor: 'bg-yellow-500/10',
  },
  runtime_error: {
    icon: AlertCircle,
    label: 'Runtime Error',
    color: 'text-orange-500',
    bgColor: 'bg-orange-500/10',
  },
  pending: {
    icon: Clock,
    label: 'Pending',
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
  if (!status) {
    return (
      <Card className="h-full">
        <CardHeader>
          <CardTitle>Result</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-center h-32 text-muted-foreground">
            Submit your code to see results
          </div>
        </CardContent>
      </Card>
    );
  }

  if (status === 'judging') {
    return (
      <Card className="h-full">
        <CardHeader>
          <CardTitle>Result</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex items-center justify-center h-32">
            <div className="flex flex-col items-center gap-2">
              <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-primary"></div>
              <p className="text-sm text-muted-foreground">Judging...</p>
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
          <CardTitle>Result</CardTitle>
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
            {totalTime !== undefined && <div>Time: {totalTime}ms</div>}
            {totalMemory !== undefined && <div>Memory: {totalMemory}MB</div>}
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
                    <div className="font-medium">Test Case #{testCase.id}</div>
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
            No test results available
          </div>
        )}
      </CardContent>
    </Card>
  );
}
