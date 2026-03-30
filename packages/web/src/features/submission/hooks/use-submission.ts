import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { Submission } from '@broccoli/web-sdk/submission';
import { useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';

const POLL_INTERVAL_MS = 1000;

const TERMINAL_STATUSES = new Set([
  'Judged',
  'CompilationError',
  'SystemError',
]);

export interface SubmissionError {
  code: string;
  message: string;
  details?: Record<string, unknown>;
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

  if (typeof value !== 'object') {
    return null;
  }

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
interface UseSubmissionOptions {
  problemId: number;
  contestId?: number;
}

interface SubmissionFile {
  filename: string;
  content: string;
}

interface UseSubmissionReturn {
  submission: Submission | null;
  isSubmitting: boolean;
  error: SubmissionError | null;
  submit: (
    files: SubmissionFile[],
    language: string,
    contestType?: string,
  ) => Promise<void>;
  reset: () => void;
}

export function useSubmission({
  problemId,
  contestId,
}: UseSubmissionOptions): UseSubmissionReturn {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();
  const [submission, setSubmission] = useState<Submission | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<SubmissionError | null>(null);
  const pollingRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const stopPolling = useCallback(() => {
    if (pollingRef.current) {
      clearInterval(pollingRef.current);
      pollingRef.current = null;
    }
  }, []);

  const startPolling = useCallback(
    (submissionId: number) => {
      stopPolling();

      pollingRef.current = setInterval(async () => {
        try {
          const { data, error: fetchError } = await apiClient.GET(
            '/submissions/{id}',
            { params: { path: { id: submissionId } } },
          );

          if (fetchError) {
            console.error('Failed to poll submission:', fetchError);
            return;
          }

          const resp = data;
          setSubmission(resp);

          if (TERMINAL_STATUSES.has(resp.status)) {
            stopPolling();
            setIsSubmitting(false);
          }
        } catch (err) {
          console.error('Polling error:', err);
        }
      }, POLL_INTERVAL_MS);
    },
    [apiClient, stopPolling],
  );

  const submit = useCallback(
    async (files: SubmissionFile[], language: string, contestType?: string) => {
      stopPolling();
      setError(null);
      setIsSubmitting(true);
      setSubmission(null);

      try {
        let data: Submission;

        const idempotencyHeaders = {
          'Idempotency-Key': crypto.randomUUID(),
        };

        if (contestId) {
          const res = await apiClient.POST(
            '/contests/{id}/problems/{problem_id}/submissions',
            {
              params: {
                path: { id: contestId, problem_id: problemId },
              },
              body: { files, language },
              headers: idempotencyHeaders,
            },
          );
          if (res.error) throw res.error;
          data = res.data;
        } else {
          const res = await apiClient.POST('/problems/{id}/submissions', {
            params: { path: { id: problemId } },
            body: { files, language, contest_type: contestType },
            headers: idempotencyHeaders,
          });
          if (res.error) throw res.error;
          data = res.data;
        }

        setSubmission(data);
        if (contestId) {
          await queryClient.invalidateQueries({
            queryKey: ['contest-submissions-table', String(contestId)],
          });
          await queryClient.invalidateQueries({
            queryKey: ['contest-submission-languages', contestId],
          });
        }
        toast.success(t('toast.submission.submitted'));
        startPolling(data.id);
      } catch (err) {
        console.error('Submission failed:', err);
        setError(parseSubmissionError(err));
        toast.error(t('toast.submission.error'));
        setIsSubmitting(false);
      }
    },
    [
      apiClient,
      contestId,
      problemId,
      queryClient,
      startPolling,
      stopPolling,
      t,
    ],
  );

  const reset = useCallback(() => {
    stopPolling();
    setSubmission(null);
    setIsSubmitting(false);
    setError(null);
  }, [stopPolling]);

  // Reset when problem changes (e.g. navigating between problems in a contest)
  useEffect(() => {
    reset();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [problemId]);

  // Cleanup on unmount
  useEffect(() => {
    return () => stopPolling();
  }, [stopPolling]);

  return { submission, isSubmitting, error, submit, reset };
}
