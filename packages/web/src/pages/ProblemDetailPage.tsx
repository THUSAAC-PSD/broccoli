import { useState } from 'react';

import type { ProblemResponse } from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { Slot } from '@broccoli/sdk/react';
import { useQuery } from '@tanstack/react-query';
import { useParams } from 'react-router';

import { CodeEditor } from '@/components/CodeEditor';
import { Markdown } from '@/components/Markdown';
import { ProblemHeader } from '@/components/ProblemHeader';
import { SubmissionResult } from '@/components/SubmissionResult';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Skeleton } from '@/components/ui/skeleton';
import { useSubmission } from '@/hooks/use-submission';

function formatMemoryLimit(kb: number) {
  if (!Number.isFinite(kb)) return '';
  const mb = kb / 1024;
  return `${mb.toFixed(mb >= 10 ? 0 : 1)} MB`;
}

export function ProblemDetailPage() {
  const { t } = useTranslation();
  const { problemId, contestId } = useParams();
  const id = Number(problemId);
  const cId = contestId ? Number(contestId) : undefined;

  const [isProblemFullscreen] = useState(false);
  const [isCodeFullscreen, setIsCodeFullscreen] = useState(false);
  const apiClient = useApiClient();

  const {
    data: problem,
    isLoading,
    error,
  } = useQuery({
    queryKey: ['problem', id],
    enabled: Number.isFinite(id),
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/problems/{id}', {
        params: { path: { id } },
      });
      if (error) throw error;
      return data as ProblemResponse;
    },
  });

  const {
    submission,
    isSubmitting,
    error: submitError,
    submit,
  } = useSubmission({ problemId: id, contestId: cId });

  if (!problemId || Number.isNaN(id)) {
    return (
      <div className="flex flex-col gap-4 p-6">
        <h1 className="text-2xl font-bold">{t('problem.notFound')}</h1>
      </div>
    );
  }

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
        <Slot name="problem-detail.header" as="div" />
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
              onSubmit={submit}
              onRun={submit}
              isFullscreen={isCodeFullscreen}
              onToggleFullscreen={() => setIsCodeFullscreen(!isCodeFullscreen)}
              storageKey={
                cId ? `contest-${cId}-problem-${id}` : `problem-${id}`
              }
            />
            <SubmissionResult
              submission={submission}
              isSubmitting={isSubmitting}
              error={submitError}
            />
            <Slot name="problem-detail.sidebar" as="div" />
          </div>
        )}
      </div>
    </div>
  );
}
