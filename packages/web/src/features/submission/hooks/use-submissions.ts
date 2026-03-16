import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { Submission } from '@broccoli/web-sdk/submission';
import { useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';

import type { SubmissionError } from './use-submission';

const POLL_INTERVAL_MS = 1000;
const TERMINAL_STATUSES = new Set([
  'Judged',
  'CompilationError',
  'SystemError',
]);

export interface SubmissionEntry {
  /** Unique client-side ID for this entry */
  id: number;
  submission: Submission | null;
  status: 'submitting' | 'polling' | 'done' | 'error';
  error: SubmissionError | null;
}

interface SubmissionFile {
  filename: string;
  content: string;
}

export interface UseSubmissionsReturn {
  entries: SubmissionEntry[];
  submit: (
    files: SubmissionFile[],
    language: string,
    contestType?: string,
  ) => Promise<void>;
  isAnySubmitting: boolean;
  activeEntryId: number | null;
  setActiveEntryId: (id: number | null) => void;
}

function parseSubmissionErrorValue(value: unknown): SubmissionError | null {
  if (!value) return null;
  if (typeof value === 'string') {
    try {
      return parseSubmissionErrorValue(JSON.parse(value));
    } catch {
      return null;
    }
  }
  if (typeof value !== 'object') return null;
  const obj = value as Record<string, unknown>;
  if (typeof obj.code === 'string' && typeof obj.message === 'string') {
    const result: SubmissionError = { code: obj.code, message: obj.message };
    if (obj.details != null && typeof obj.details === 'object') {
      result.details = obj.details as Record<string, unknown>;
    }
    return result;
  }
  return (
    parseSubmissionErrorValue(obj.error) ??
    parseSubmissionErrorValue(obj.data) ??
    parseSubmissionErrorValue(obj.body) ??
    parseSubmissionErrorValue(obj.response)
  );
}

function parseSubmissionError(err: unknown): SubmissionError {
  const parsed = parseSubmissionErrorValue(err);
  if (parsed) return parsed;
  return { code: 'UNKNOWN', message: String(err) };
}

let entryIdCounter = 0;

interface UseSubmissionsOptions {
  problemId: number;
  contestId?: number;
}

export function useSubmissions({
  problemId,
  contestId,
}: UseSubmissionsOptions): UseSubmissionsReturn {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();
  const [entries, setEntries] = useState<SubmissionEntry[]>([]);
  const [activeEntryId, setActiveEntryId] = useState<number | null>(null);
  const pollersRef = useRef<Map<number, ReturnType<typeof setInterval>>>(
    new Map(),
  );

  const stopPolling = useCallback((entryId: number) => {
    const interval = pollersRef.current.get(entryId);
    if (interval) {
      clearInterval(interval);
      pollersRef.current.delete(entryId);
    }
  }, []);

  const stopAllPolling = useCallback(() => {
    for (const [id, interval] of Array.from(pollersRef.current)) {
      clearInterval(interval);
      pollersRef.current.delete(id);
    }
  }, []);

  const startPolling = useCallback(
    (entryId: number, submissionId: number) => {
      stopPolling(entryId);

      const interval = setInterval(async () => {
        try {
          const { data, error: fetchError } = await apiClient.GET(
            '/submissions/{id}',
            { params: { path: { id: submissionId } } },
          );
          if (fetchError) {
            console.error('Failed to poll submission:', fetchError);
            return;
          }

          setEntries((prev) =>
            prev.map((e) => {
              if (e.id !== entryId) return e;
              const isDone = TERMINAL_STATUSES.has(data.status);
              return {
                ...e,
                submission: data,
                status: isDone ? 'done' : 'polling',
              };
            }),
          );

          if (TERMINAL_STATUSES.has(data.status)) {
            stopPolling(entryId);
          }
        } catch (err) {
          console.error('Polling error:', err);
        }
      }, POLL_INTERVAL_MS);

      pollersRef.current.set(entryId, interval);
    },
    [apiClient, stopPolling],
  );

  const submit = useCallback(
    async (files: SubmissionFile[], language: string, contestType?: string) => {
      const entryId = ++entryIdCounter;

      const newEntry: SubmissionEntry = {
        id: entryId,
        submission: null,
        status: 'submitting',
        error: null,
      };

      setEntries((prev) => [newEntry, ...prev]);
      setActiveEntryId(entryId);

      try {
        let data: Submission;

        if (contestId) {
          const res = await apiClient.POST(
            '/contests/{id}/problems/{problem_id}/submissions',
            {
              params: {
                path: { id: contestId, problem_id: problemId },
              },
              body: { files, language },
            },
          );
          if (res.error) throw res.error;
          data = res.data;
        } else {
          const res = await apiClient.POST('/problems/{id}/submissions', {
            params: { path: { id: problemId } },
            body: { files, language, contest_type: contestType },
          });
          if (res.error) throw res.error;
          data = res.data;
        }

        setEntries((prev) =>
          prev.map((e) =>
            e.id === entryId
              ? { ...e, submission: data, status: 'polling' as const }
              : e,
          ),
        );

        if (contestId) {
          queryClient.invalidateQueries({
            queryKey: ['contest-submissions-table', String(contestId)],
          });
          queryClient.invalidateQueries({
            queryKey: ['contest-submission-languages', contestId],
          });
        }

        toast.success(t('toast.submission.submitted'));
        startPolling(entryId, data.id);
      } catch (err) {
        console.error('Submission failed:', err);
        const submissionError = parseSubmissionError(err);
        setEntries((prev) =>
          prev.map((e) =>
            e.id === entryId
              ? { ...e, status: 'error' as const, error: submissionError }
              : e,
          ),
        );
        toast.error(t('toast.submission.error'));
      }
    },
    [apiClient, contestId, problemId, queryClient, startPolling, t],
  );

  const isAnySubmitting = entries.some(
    (e) => e.status === 'submitting' || e.status === 'polling',
  );

  // Reset when problem changes
  useEffect(() => {
    stopAllPolling();
    setEntries([]);
    setActiveEntryId(null);
  }, [problemId, stopAllPolling]);

  // Cleanup on unmount
  useEffect(() => {
    return () => stopAllPolling();
  }, [stopAllPolling]);

  return { entries, submit, isAnySubmitting, activeEntryId, setActiveEntryId };
}
