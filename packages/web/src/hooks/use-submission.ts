import { useCallback, useEffect, useRef, useState } from 'react';

import type { SubmissionResponse } from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';

const POLL_INTERVAL_MS = 1000;
const POLL_TIMEOUT_MS = 60_000;

const TERMINAL_STATUSES = new Set([
  'Judged',
  'CompilationError',
  'SystemError',
]);

const FILENAME_MAP: Record<string, string> = {
  cpp: 'solution.cpp',
  c: 'solution.c',
  python: 'solution.py',
  java: 'Main.java',
};

interface UseSubmissionOptions {
  problemId: number;
  contestId?: number;
}

interface UseSubmissionReturn {
  submission: SubmissionResponse | null;
  isSubmitting: boolean;
  error: string | null;
  submit: (code: string, language: string) => Promise<void>;
  reset: () => void;
}

export function useSubmission({
  problemId,
  contestId,
}: UseSubmissionOptions): UseSubmissionReturn {
  const apiClient = useApiClient();
  const [submission, setSubmission] = useState<SubmissionResponse | null>(null);
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

          const resp = data as SubmissionResponse;
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
    async (code: string, language: string) => {
      stopPolling();
      setError(null);
      setIsSubmitting(true);
      setSubmission(null);

      const filename = FILENAME_MAP[language] ?? `solution.${language}`;
      const body = {
        files: [{ filename, content: code }],
        language,
      };

      try {
        let data: SubmissionResponse;

        if (contestId) {
          const res = await apiClient.POST(
            '/contests/{id}/problems/{problem_id}/submissions',
            {
              params: { path: { contest_id: contestId, problem_id: problemId } },
              body,
            },
          );
          if (res.error) throw res.error;
          data = res.data as SubmissionResponse;
        } else {
          const res = await apiClient.POST('/problems/{id}/submissions', {
            params: { path: { id: problemId } },
            body,
          });
          if (res.error) throw res.error;
          data = res.data as SubmissionResponse;
        }

        setSubmission(data);
        startPolling(data.id);
      } catch (err) {
        console.error('Submission failed:', err);
        setError(String(err));
        setIsSubmitting(false);
      }
    },
    [apiClient, contestId, problemId, startPolling, stopPolling],
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
