import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Slot } from '@broccoli/web-sdk/slot';
import { Button, Skeleton } from '@broccoli/web-sdk/ui';
import { formatBytes, formatKibibytes } from '@broccoli/web-sdk/utils';
import { useQuery } from '@tanstack/react-query';
import { ArrowLeft, Check, Code2, Copy, Edit } from 'lucide-react';
import { useEffect, useState } from 'react';

import { CodeEditor, type EditorFile } from '@/components/CodeEditor';
import { Markdown } from '@/components/Markdown';
import { useAuth } from '@/features/auth/hooks/use-auth';
import { ContestCountdownMini } from '@/features/contest/components/ContestCountdown';
import { ProblemHeader } from '@/features/problem/components/ProblemHeader';
import { SubmissionResult } from '@/features/submission/components/SubmissionResult';
import { useSubmission } from '@/features/submission/hooks/use-submission';

import { ProblemEditForm } from './ProblemEditForm';

const INLINE_SAMPLE_MAX_SIZE = 1024;
type SampleContentMap = Record<number, { input?: string; output?: string }>;
type CopiedNotice = { text: string; top: number; left: number } | null;

interface ProblemViewProps {
  problemId: number;
  contestId?: number;
}

export default function ProblemView({
  problemId,
  contestId,
}: ProblemViewProps) {
  const { t } = useTranslation();
  const { user } = useAuth();

  const [isCodeFullscreen, setIsCodeFullscreen] = useState(false);
  const [showCodingPanel, setShowCodingPanel] = useState(false);
  const [showEditPage, setShowEditPage] = useState(false);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [copiedNotice, setCopiedNotice] = useState<CopiedNotice>(null);
  const apiClient = useApiClient();

  const handleBackToDescription = () => {
    setShowCodingPanel(false);
    setShowEditPage(false);
  };

  useEffect(() => {
    setShowCodingPanel(false);
    setShowEditPage(false);
    setIsCodeFullscreen(false);
    setCopiedKey(null);
    setCopiedNotice(null);
  }, [problemId]);

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
      return data;
    },
  });

  const { data: contestProblems = [] } = useQuery({
    queryKey: ['contest-problems', contestId],
    enabled: Number.isFinite(contestId),
    queryFn: async () => {
      if (!contestId) return [];
      const { data, error } = await apiClient.GET('/contests/{id}/problems', {
        params: { path: { id: contestId } },
      });
      if (error || !data) return [];
      return data;
    },
  });

  const { data: sampleContents = {} as SampleContentMap } =
    useQuery<SampleContentMap>({
      queryKey: [
        'problem-sample-contents',
        problemId,
        problem?.samples.map((sample) => [
          sample.id,
          sample.input_size,
          sample.output_size,
        ]),
      ],
      enabled: Number.isFinite(problemId) && !!problem,
      queryFn: async () => {
        if (!problem) return {} as SampleContentMap;

        const entries = await Promise.all(
          problem.samples.map(async (sample) => {
            const shouldLoadInput = sample.input_size <= INLINE_SAMPLE_MAX_SIZE;
            const shouldLoadOutput =
              sample.output_size <= INLINE_SAMPLE_MAX_SIZE;

            if (!shouldLoadInput && !shouldLoadOutput) {
              return [sample.id, {}] as const;
            }

            const { data, error } = await apiClient.GET(
              '/problems/{id}/test-cases/{tc_id}',
              {
                params: { path: { id: problemId, tc_id: sample.id } },
              },
            );
            if (error || !data) return [sample.id, {}] as const;

            return [
              sample.id,
              {
                input: shouldLoadInput ? data.input : undefined,
                output: shouldLoadOutput ? data.expected_output : undefined,
              },
            ] as const;
          }),
        );

        return Object.fromEntries(entries) as SampleContentMap;
      },
    });

  const {
    submission,
    isSubmitting,
    error: submitError,
    submit: rawSubmit,
  } = useSubmission({ problemId: problemId, contestId: contestId });

  const handleSubmit = (files: EditorFile[], language: string) => {
    rawSubmit(
      files.map(({ filename, content }) => ({ filename, content })),
      language,
    );
  };

  if (!problemId || Number.isNaN(problemId)) {
    return (
      <div className="flex flex-col gap-4 p-6">
        <h1 className="text-2xl font-bold">{t('problem.notFound')}</h1>
      </div>
    );
  }

  const contestProblemLabel = contestId
    ? contestProblems.find((item) => item.problem_id === problemId)?.label
    : undefined;
  const headerId = contestProblemLabel ?? (problem ? String(problem.id) : '—');
  const timeLimit = problem ? `${problem.time_limit} ms` : '—';
  const memoryLimit = problem ? formatKibibytes(problem.memory_limit) : '—';

  const getSampleCaseData = async (tcId: number) => {
    const { data, error } = await apiClient.GET(
      '/problems/{id}/test-cases/{tc_id}',
      {
        params: { path: { id: problemId, tc_id: tcId } },
      },
    );
    if (error || !data) return null;
    return data;
  };

  const downloadSampleFile = async (
    tcId: number,
    sampleIndex: number,
    type: 'input' | 'output',
  ) => {
    const data = await getSampleCaseData(tcId);
    if (!data) return;

    const content = type === 'input' ? data.input : data.expected_output;
    const ext = type === 'input' ? 'in' : 'out';
    const fileName = `sample${sampleIndex}.${ext}`;
    const blob = new Blob([content], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);

    const link = document.createElement('a');
    link.href = url;
    link.download = fileName;
    link.click();

    URL.revokeObjectURL(url);
  };

  const copySampleFile = async (
    tcId: number,
    sampleIndex: number,
    type: 'input' | 'output',
    anchorEl: HTMLElement,
    inlineContent?: string,
  ) => {
    let content = inlineContent;

    if (content === undefined) {
      const data = await getSampleCaseData(tcId);
      if (!data) return;
      content = type === 'input' ? data.input : data.expected_output;
    }

    await navigator.clipboard.writeText(content);

    const key = `${type}-${tcId}`;
    setCopiedKey(key);
    const ext = type === 'input' ? 'in' : 'out';
    const rect = anchorEl.getBoundingClientRect();
    setCopiedNotice({
      text: t('problem.copied', { file: `sample${sampleIndex}.${ext}` }),
      top: rect.top - 8,
      left: rect.left + rect.width / 2,
    });

    window.setTimeout(() => {
      setCopiedKey((current) => (current === key ? null : current));
    }, 1200);

    window.setTimeout(() => {
      setCopiedNotice((current) =>
        current?.text ===
        t('problem.copied', { file: `sample${sampleIndex}.${ext}` })
          ? null
          : current,
      );
    }, 1500);
  };

  // Shared description card body, used in both views
  const descriptionBody = isLoading ? (
    <div className="space-y-3">
      <Skeleton className="h-5 w-64" />
      <Skeleton className="h-5 w-48" />
      <Skeleton className="h-24 w-full" />
    </div>
  ) : error ? (
    <div className="text-sm text-destructive">{t('problem.loadError')}</div>
  ) : problem ? (
    <div className="prose prose-sm dark:prose-invert max-w-none">
      <Markdown>{problem.content}</Markdown>

      {problem.samples.length > 0 && (
        <section className="mt-6 space-y-4">
          <h3 className="text-base font-bold">{t('problem.examples')}</h3>

          {problem.samples.map((sample, index) => {
            const sampleNumber = index + 1;
            const sampleContent = sampleContents[sample.id] ?? {};

            return (
              <div key={sample.id} className="space-y-3">
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                  <div className="space-y-2">
                    <div className="flex items-center justify-between px-1 text-sm font-medium">
                      {`${t('problem.input')} #${sampleNumber}`}
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        title={t('problem.copy')}
                        onClick={(event) =>
                          copySampleFile(
                            sample.id,
                            sampleNumber,
                            'input',
                            event.currentTarget,
                            sampleContent.input,
                          )
                        }
                      >
                        {copiedKey === `input-${sample.id}` ? (
                          <Check className="h-4 w-4" />
                        ) : (
                          <Copy className="h-4 w-4" />
                        )}
                      </Button>
                    </div>
                    <div className="border rounded-lg overflow-hidden">
                      {sampleContent.input !== undefined ? (
                        <pre className="p-4 text-sm font-mono overflow-x-auto mb-0">
                          {sampleContent.input}
                        </pre>
                      ) : (
                        <div className="p-4 text-sm">
                          <button
                            type="button"
                            className="text-primary underline underline-offset-2 decoration-primary/40 hover:decoration-primary/80 transition-colors"
                            onClick={() =>
                              downloadSampleFile(
                                sample.id,
                                sampleNumber,
                                'input',
                              )
                            }
                          >
                            {t('problem.downloadSampleFile', {
                              file: `sample${sampleNumber}.in`,
                              size: formatBytes(sample.input_size),
                            })}
                          </button>
                        </div>
                      )}
                    </div>
                  </div>

                  <div className="space-y-2">
                    <div className="flex items-center justify-between px-1 text-sm font-medium">
                      {`${t('problem.output')} #${sampleNumber}`}
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        title={t('problem.copy')}
                        onClick={(event) =>
                          copySampleFile(
                            sample.id,
                            sampleNumber,
                            'output',
                            event.currentTarget,
                            sampleContent.output,
                          )
                        }
                      >
                        {copiedKey === `output-${sample.id}` ? (
                          <Check className="h-4 w-4" />
                        ) : (
                          <Copy className="h-4 w-4" />
                        )}
                      </Button>
                    </div>
                    <div className="border rounded-lg overflow-hidden">
                      {sampleContent.output !== undefined ? (
                        <pre className="p-4 text-sm font-mono overflow-x-auto mb-0">
                          {sampleContent.output}
                        </pre>
                      ) : (
                        <div className="p-4 text-sm">
                          <button
                            type="button"
                            className="text-primary underline underline-offset-2 decoration-primary/40 hover:decoration-primary/80 transition-colors"
                            onClick={() =>
                              downloadSampleFile(
                                sample.id,
                                sampleNumber,
                                'output',
                              )
                            }
                          >
                            {t('problem.downloadSampleFile', {
                              file: `sample${sampleNumber}.out`,
                              size: formatBytes(sample.output_size),
                            })}
                          </button>
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              </div>
            );
          })}
        </section>
      )}
    </div>
  ) : null;

  return (
    <div className="flex flex-col flex-1 min-h-0">
      {copiedNotice && (
        <div
          className="fixed z-50 -translate-x-1/2 -translate-y-full rounded-md border bg-background px-3 py-1.5 text-sm shadow-sm"
          style={{ top: copiedNotice.top, left: copiedNotice.left }}
        >
          {copiedNotice.text || t('problem.copiedSimple')}
        </div>
      )}

      {/* ── Fixed header section (never scrolls) ── */}
      <div className="flex-shrink-0 px-6 pt-3 pb-0">
        <div className="flex items-start sm:items-center gap-4">
          <div className="min-w-0 flex-1">
            <ProblemHeader
              id={headerId}
              title={problem?.title ?? t('problem.title')}
              type="Default"
              io="Standard Input / Output"
              timeLimit={timeLimit}
              memoryLimit={memoryLimit}
            />
          </div>
          <div className="hidden lg:flex items-center gap-4">
            <ContestCountdownMini />
          </div>
        </div>
      </div>

      {/* ── Fixed action bar (never scrolls) ── */}
      <div className="flex-shrink-0 px-6 py-1.5 border-b flex items-center justify-between bg-background">
        {!showCodingPanel && !showEditPage ? (
          <>
            <span className="text-sm font-semibold text-foreground">
              {t('problem.description')}
            </span>
            <div className="flex items-center gap-2">
              {user && user.permissions.includes('problem:edit') && (
                <Button
                  onClick={() => setShowEditPage(true)}
                  size="sm"
                  variant="default"
                  className="gap-1.5 h-8 px-4 font-semibold bg-primary text-primary-foreground hover:bg-primary/90 shadow-sm"
                >
                  <Edit className="h-3.5 w-3.5" />
                  {t('problem.edit')}
                </Button>
              )}
              <Button
                onClick={() => setShowCodingPanel(true)}
                size="sm"
                variant="default"
                className="gap-1.5 h-8 px-4 font-semibold bg-primary text-primary-foreground hover:bg-primary/90 shadow-sm"
              >
                <Code2 className="h-3.5 w-3.5" />
                {t('problem.startCoding')}
              </Button>
            </div>
          </>
        ) : (
          <>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => handleBackToDescription()}
              className="gap-1.5 -ml-2 h-8 text-sm font-semibold text-foreground hover:text-foreground"
            >
              <ArrowLeft className="h-3.5 w-3.5" />
              {t('problem.backToDescription')}
            </Button>
            {/* placeholder keeps bar height identical to description bar */}
            <div className="h-8" />
          </>
        )}
      </div>

      {/* ── Scrollable / flexible content area ── */}
      {!showCodingPanel && !showEditPage && (
        <div className="flex-1 overflow-y-auto p-6">
          <div className="max-w-4xl mx-auto">{descriptionBody}</div>
        </div>
      )}

      {showCodingPanel && (
        <div className="flex-1 grid grid-cols-1 lg:grid-cols-2 gap-6 p-6 overflow-hidden">
          <div
            className={`flex flex-col overflow-hidden ${isCodeFullscreen ? 'col-span-2' : ''}`}
          >
            <CodeEditor
              onSubmit={handleSubmit}
              onRun={handleSubmit}
              isFullscreen={isCodeFullscreen}
              onToggleFullscreen={() => setIsCodeFullscreen(!isCodeFullscreen)}
              storageKey={
                contestId
                  ? `contest-${contestId}-problem-${problemId}`
                  : `problem-${problemId}`
              }
              submissionFormat={problem?.submission_format}
            />
          </div>

          {!isCodeFullscreen && (
            <div className="flex flex-col gap-6 overflow-y-auto">
              <SubmissionResult
                submission={submission}
                isSubmitting={isSubmitting}
                error={submitError}
              />
              <Slot name="problem-detail.sidebar" as="div" />
            </div>
          )}
        </div>
      )}

      {showEditPage && <ProblemEditForm problemId={problemId} />}
    </div>
  );
}
