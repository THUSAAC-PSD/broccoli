import { useState } from 'react';

import { useApiClient } from '@broccoli/sdk/api';
import type { ProblemResponse } from '@broccoli/sdk';
import { useTranslation } from '@broccoli/sdk/i18n';
import { useQuery } from '@tanstack/react-query';

import { CodeEditor } from '@/components/CodeEditor';
import { Markdown } from '@/components/Markdown';
import { ProblemHeader } from '@/components/ProblemHeader';
import { SubmissionResult } from '@/components/SubmissionResult';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';

type SubmissionStatus = {
  status: 'judging' | 'completed';
  verdict?: string;
  testCases?: Array<{
    id: number;
    status:
      | 'accepted'
      | 'wrong_answer'
      | 'time_limit'
      | 'runtime_error'
      | 'pending';
    time?: number;
    memory?: number;
    message?: string;
  }>;
  totalTime?: number;
  totalMemory?: number;
};

function formatMemoryLimit(kb: number) {
  if (!Number.isFinite(kb)) return '';
  const mb = kb / 1024;
  return `${mb.toFixed(mb >= 10 ? 0 : 1)} MB`;
}

interface ProblemPageProps {
  problemId: number;
}

export function ProblemPage({ problemId }: ProblemPageProps) {
  const { t } = useTranslation();
  const apiClient = useApiClient();

  const [submissionResult, setSubmissionResult] =
    useState<SubmissionStatus | null>(null);
  const [isProblemFullscreen] = useState(false);
  const [isCodeFullscreen, setIsCodeFullscreen] = useState(false);

  const {
    data: problem,
    isLoading,
    error,
  } = useQuery({
    queryKey: ['problem', problemId],
    enabled: Number.isFinite(problemId),
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/problems/{id}', {
        params: { path: { id: problemId } },
      });
      if (error) throw error;
      return data as ProblemResponse;
    },
  });

  const handleSubmit = (code: string, language: string) => {
    console.log('Submitting code:', { code, language });

    setSubmissionResult({
      status: 'judging',
    });

    setTimeout(() => {
      // TODO: implement real submission logic
      setSubmissionResult({
        status: 'completed',
        verdict: 'Accepted',
        totalTime: 15,
        totalMemory: 2.4,
        testCases: [
          { id: 1, status: 'accepted', time: 5, memory: 1.2 },
          { id: 2, status: 'accepted', time: 10, memory: 1.2 },
        ],
      });
    }, 2000);
  };

  const handleRun = (code: string, language: string) => {
    console.log('Running code:', { code, language });

    setSubmissionResult({
      status: 'judging',
    });

    setTimeout(() => {
      // TODO: use real submission result
      setSubmissionResult({
        status: 'completed',
        verdict: 'Custom Test Passed',
        totalTime: 8,
        totalMemory: 1.5,
        testCases: [
          {
            id: 1,
            status: 'accepted',
            time: 8,
            memory: 1.5,
            message: 'Custom test case passed',
          },
        ],
      });
    }, 1500);
  };

  const headerId = problem ? String(problem.id) : '—';
  const timeLimit = problem ? `${problem.time_limit} ms` : '—';
  const memoryLimit = problem ? formatMemoryLimit(problem.memory_limit) : '—';

  return (
    <div className="flex flex-col h-full">
      <div className="p-6 pb-0">
        <ProblemHeader
          id={headerId}
          title={problem?.title ?? t('problem.title')}
          type="Default"
          io="Standard Input / Output"
          timeLimit={timeLimit}
          memoryLimit={memoryLimit}
        />
      </div>

      <div className="flex-1 grid grid-cols-1 lg:grid-cols-2 gap-6 p-6 overflow-hidden">
        {!isCodeFullscreen && (
          <div
            className={`flex flex-col gap-6 overflow-y-auto ${isProblemFullscreen ? 'col-span-2' : ''}`}
          >
            <Card className="h-full overflow-y-auto">
              <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-4">
                <CardTitle>{t('problem.description')}</CardTitle>
              </CardHeader>
              <CardContent>
                {isLoading ? (
                  <div className="space-y-3">
                    <Skeleton className="h-5 w-64" />
                    <Skeleton className="h-5 w-48" />
                    <Skeleton className="h-24 w-full" />
                  </div>
                ) : error ? (
                  <div className="text-sm text-destructive">
                    {t('problem.loadError')}
                  </div>
                ) : problem ? (
                  <div className="prose prose-sm dark:prose-invert max-w-none">
                    <Markdown>{problem.content}</Markdown>
                  </div>
                ) : null}
              </CardContent>
            </Card>
          </div>
        )}

        {!isProblemFullscreen && (
          <div
            className={`flex flex-col gap-6 overflow-y-auto ${isCodeFullscreen ? 'col-span-2' : ''}`}
          >
            <CodeEditor
              onSubmit={handleSubmit}
              onRun={handleRun}
              isFullscreen={isCodeFullscreen}
              onToggleFullscreen={() => setIsCodeFullscreen(!isCodeFullscreen)}
            />
            <SubmissionResult
              status={submissionResult?.status}
              verdict={submissionResult?.verdict}
              testCases={submissionResult?.testCases}
              totalTime={submissionResult?.totalTime}
              totalMemory={submissionResult?.totalMemory}
            />
          </div>
        )}
      </div>
    </div>
  );
}
