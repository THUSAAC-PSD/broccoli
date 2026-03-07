import type { ContestProblemResponse, ProblemResponse } from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
import { Slot } from '@broccoli/sdk/react';
import { useQuery } from '@tanstack/react-query';
import { ArrowLeft, Check, Code2, Copy } from 'lucide-react';
import { useState } from 'react';
import { useParams } from 'react-router';

import { CodeEditor } from '@/components/CodeEditor';
import { Markdown } from '@/components/Markdown';
import { ProblemHeader } from '@/components/ProblemHeader';
import { SubmissionResult } from '@/components/SubmissionResult';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { useSubmission } from '@/hooks/use-submission';

const INLINE_SAMPLE_MAX_SIZE = 1024;
type SampleContentMap = Record<number, { input?: string; output?: string }>;
type CopiedNotice = { text: string; top: number; left: number } | null;

function formatMemoryLimit(kb: number) {
  if (!Number.isFinite(kb)) return '';
  const mb = kb / 1024;
  return `${mb.toFixed(mb >= 10 ? 0 : 1)} MB`;
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function ProblemDetailPage() {
  const { t } = useTranslation();
  const { problemId, contestId } = useParams();
  const id = Number(problemId);
  const cId = contestId ? Number(contestId) : undefined;

  const [isCodeFullscreen, setIsCodeFullscreen] = useState(false);
  const [showCodingPanel, setShowCodingPanel] = useState(false);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [copiedNotice, setCopiedNotice] = useState<CopiedNotice>(null);
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

  const { data: contestProblems = [] } = useQuery({
    queryKey: ['contest-problems', cId],
    enabled: Number.isFinite(cId),
    queryFn: async () => {
      if (!cId) return [] as ContestProblemResponse[];
      const { data, error } = await apiClient.GET('/contests/{id}/problems', {
        params: { path: { id: cId } },
      });
      if (error || !data) return [] as ContestProblemResponse[];
      return data as ContestProblemResponse[];
    },
  });

  const { data: sampleContents = {} as SampleContentMap } =
    useQuery<SampleContentMap>({
      queryKey: [
        'problem-sample-contents',
        id,
        problem?.samples.map((sample) => [
          sample.id,
          sample.input_size,
          sample.output_size,
        ]),
      ],
      enabled: Number.isFinite(id) && !!problem,
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
                params: { path: { id, tc_id: sample.id } },
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
    submit,
  } = useSubmission({ problemId: id, contestId: cId });

  if (!problemId || Number.isNaN(id)) {
    return (
      <div className="flex flex-col gap-4 p-6">
        <h1 className="text-2xl font-bold">{t('problem.notFound')}</h1>
      </div>
    );
  }

  const contestProblemLabel = cId
    ? contestProblems.find((item) => item.problem_id === id)?.label
    : undefined;
  const headerId = contestProblemLabel ?? (problem ? String(problem.id) : '—');
  const timeLimit = problem ? `${problem.time_limit} ms` : '—';
  const memoryLimit = problem ? formatMemoryLimit(problem.memory_limit) : '—';

  const getSampleCaseData = async (tcId: number) => {
    const { data, error } = await apiClient.GET(
      '/problems/{id}/test-cases/{tc_id}',
      {
        params: { path: { id, tc_id: tcId } },
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

      {/* -- Fixed header section (never scrolls) -- */}
      <div className="flex-shrink-0 px-6 pt-3 pb-0 relative">
        <ProblemHeader
          id={headerId}
          title={problem?.title ?? t('problem.title')}
          type="Default"
          io="Standard Input / Output"
          timeLimit={timeLimit}
          memoryLimit={memoryLimit}
        />
        <Slot name="problem-detail.header" as="div" className="relative" />
      </div>

      {/* -- Fixed action bar (never scrolls) -- */}
      <div className="flex-shrink-0 px-6 py-1.5 border-b flex items-center justify-between bg-background">
        {!showCodingPanel ? (
          <>
            <span className="text-sm font-semibold text-foreground">
              {t('problem.description')}
            </span>
            <Button
              onClick={() => setShowCodingPanel(true)}
              size="sm"
              variant="default"
              className="gap-1.5 h-8 px-4 font-semibold bg-primary text-primary-foreground hover:bg-primary/90 shadow-sm"
            >
              <Code2 className="h-3.5 w-3.5" />
              {t('problem.startCoding')}
            </Button>
          </>
        ) : (
          <>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setShowCodingPanel(false)}
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

      {/* -- Scrollable / flexible content area -- */}
      {!showCodingPanel ? (
        <div className="flex-1 overflow-y-auto p-6">
          <div className="max-w-4xl mx-auto">{descriptionBody}</div>
        </div>
      ) : (
        <div className="flex-1 grid grid-cols-1 lg:grid-cols-2 gap-6 p-6 overflow-hidden">
          <div
            className={`flex flex-col overflow-hidden ${isCodeFullscreen ? 'col-span-2' : ''}`}
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
    </div>
  );
}
