import { useApiClient } from '@broccoli/web-sdk/api';
import type { Submission } from '@broccoli/web-sdk/submission';
import { useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';

const POLL_INTERVAL_MS = 1000;
const POLL_TIMEOUT_MS = 60_000;

const TERMINAL_STATUSES = new Set([
  'Judged',
  'CompilationError',
  'SystemError',
]);

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
  error: string | null;
  submit: (files: SubmissionFile[], language: string) => Promise<void>;
  reset: () => void;
}

export function useSubmission({
  problemId,
  contestId,
}: UseSubmissionOptions): UseSubmissionReturn {
  const apiClient = useApiClient();
  const queryClient = useQueryClient();
  const [submission, setSubmission] = useState<Submission | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pollingRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const stopPolling = useCallback(() => {
    if (pollingRef.current) {
      clearInterval(pollingRef.current);
      pollingRef.current = null;
    }
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
    }
  }, []);

  const startPolling = useCallback(
    (submissionId: number) => {
      stopPolling();

      // Stop polling after timeout
      timeoutRef.current = setTimeout(() => {
        stopPolling();
        setIsSubmitting(false);
      }, POLL_TIMEOUT_MS);

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
    async (files: SubmissionFile[], language: string) => {
      stopPolling();
      setError(null);
      setIsSubmitting(true);
      setSubmission(null);

      const body = {
        files,
        language,
      };

      try {
        let data: Submission;

        if (contestId) {
          const res = await apiClient.POST(
            '/contests/{id}/problems/{problem_id}/submissions',
            {
              params: {
                path: { id: contestId, problem_id: problemId },
              },
              body,
            },
          );
          if (res.error) throw res.error;
          data = res.data;
        } else {
          const res = await apiClient.POST('/problems/{id}/submissions', {
            params: { path: { id: problemId } },
            body,
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
        toast.success('Code submitted successfully.');
        startPolling(data.id);
      } catch (err) {
        console.error('Submission failed:', err);
        setError(String(err));
        toast.error('Failed to submit code. Please try again.');
        setIsSubmitting(false);
      }
    },
    [apiClient, contestId, problemId, queryClient, startPolling, stopPolling],
  );

  const reset = useCallback(() => {
    stopPolling();
    setSubmission(null);
    setIsSubmitting(false);
    setError(null);
  }, [stopPolling]);

  // Cleanup on unmount
  useEffect(() => {
    return () => stopPolling();
  }, [stopPolling]);

  return { submission, isSubmitting, error, submit, reset };
}
