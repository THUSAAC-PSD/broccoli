import { useApiClient } from '@broccoli/web-sdk/api';
import { useRegistries } from '@broccoli/web-sdk/hooks';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import type { DockviewApi } from 'dockview-react';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type { EditorFile } from '@/components/CodeEditor';
import { useSubmissions } from '@/features/submission/hooks/use-submissions';

import {
  type ProblemDockContextValue,
  ProblemDockProvider,
} from './dock/ProblemDockContext';
import { ProblemDockLayout } from './dock/ProblemDockLayout';
import { ProblemMobileLayout } from './dock/ProblemMobileLayout';
import { useBreakpoint } from './dock/use-breakpoint';
import { ProblemEditForm } from './ProblemEditForm';
import { ProblemViewHeader } from './ProblemViewHeader';

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
  const breakpoint = useBreakpoint();
  const isDesktop = breakpoint === 'desktop';
  const dockApiRef = useRef<DockviewApi | null>(null);

  const [showEditPage, setShowEditPage] = useState(false);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [copiedNotice, setCopiedNotice] = useState<CopiedNotice>(null);
  const [contestType, setContestType] = useState<string | undefined>(undefined);
  const apiClient = useApiClient();
  const { data: registries } = useRegistries();

  useEffect(() => {
    setShowEditPage(false);
    setCopiedKey(null);
    setCopiedNotice(null);
    setContestType(undefined);
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

  const submissions = useSubmissions({ problemId, contestId });

  const getSampleCaseData = useCallback(
    async (tcId: number) => {
      const { data, error } = await apiClient.GET(
        '/problems/{id}/test-cases/{tc_id}',
        { params: { path: { id: problemId, tc_id: tcId } } },
      );
      if (error || !data) return null;
      return data;
    },
    [apiClient, problemId],
  );

  const onDownloadSample = useCallback(
    async (tcId: number, sampleIndex: number, type: 'input' | 'output') => {
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
    },
    [getSampleCaseData],
  );

  const onCopySample = useCallback(
    async (
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

      try {
        await navigator.clipboard.writeText(content);
      } catch {
        return;
      }

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
    },
    [getSampleCaseData, t],
  );

  // Use the first available registry option as fallback
  // TODO: handle the case when the registry is empty more gracefully
  const fallbackContestType = registries?.contest_types?.[0] ?? '';
  const effectiveContestType =
    contestType ?? problem?.default_contest_type ?? fallbackContestType;

  const handleSubmit = useCallback(
    (files: EditorFile[], language: string) => {
      submissions.submit(
        files.map(({ filename, content }) => ({ filename, content })),
        language,
        effectiveContestType,
      );
    },
    [submissions, effectiveContestType],
  );

  const storageKey = contestId
    ? `contest-${contestId}-problem-${problemId}`
    : `problem-${problemId}`;

  const contestProblemLabel = contestId
    ? contestProblems.find((item) => item.problem_id === problemId)?.label
    : undefined;
  const headerId = contestProblemLabel ?? (problem ? String(problem.id) : '—');

  const latestEntry = submissions.entries[0] ?? null;
  const latestSubmission = latestEntry?.submission ?? null;

  const contextValue = useMemo<ProblemDockContextValue>(
    () => ({
      problem,
      isLoading,
      error: error as Error | null,
      sampleContents,
      copiedKey,
      onCopySample,
      onDownloadSample,
      submissions,
      onSubmit: handleSubmit,
      onRun: handleSubmit,
      storageKey,
      contestType: effectiveContestType,
      onContestTypeChange: !contestId ? setContestType : undefined,
      contestTypes: !contestId ? (registries?.contest_types ?? []) : undefined,
      submissionFormat: problem?.submission_format,
      contestId,
      problemId,
      latestSubmission,
      latestEntry,
    }),
    [
      problem,
      isLoading,
      error,
      sampleContents,
      copiedKey,
      onCopySample,
      onDownloadSample,
      submissions,
      handleSubmit,
      storageKey,
      effectiveContestType,
      contestId,
      problemId,
      registries?.contest_types,
      latestSubmission,
      latestEntry,
    ],
  );

  if (!problemId || Number.isNaN(problemId)) {
    return (
      <div className="flex flex-col gap-4 p-6">
        <h1 className="text-2xl font-bold">{t('problem.notFound')}</h1>
      </div>
    );
  }

  if (showEditPage) {
    return (
      <div className="flex flex-col flex-1 min-h-0">
        <ProblemViewHeader
          problem={problem}
          headerId={headerId}
          contestId={contestId}
          onEdit={() => setShowEditPage(true)}
        />
        <ProblemEditForm problemId={problemId} />
      </div>
    );
  }

  return (
    <ProblemDockProvider value={contextValue}>
      <div className="flex flex-col flex-1 min-h-0">
        {copiedNotice && (
          <div
            className="fixed z-50 -translate-x-1/2 -translate-y-full rounded-md border bg-background px-3 py-1.5 text-sm shadow-xs"
            style={{ top: copiedNotice.top, left: copiedNotice.left }}
          >
            {copiedNotice.text || t('problem.copiedSimple')}
          </div>
        )}

        <ProblemViewHeader
          problem={problem}
          headerId={headerId}
          contestId={contestId}
          onEdit={() => setShowEditPage(true)}
        />

        {isDesktop ? (
          <ProblemDockLayout dockApiRef={dockApiRef} />
        ) : (
          <ProblemMobileLayout problemId={problemId} />
        )}
      </div>
    </ProblemDockProvider>
  );
}
