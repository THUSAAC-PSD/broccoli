/**
 * Fetches test cases for a problem from the platform API.
 */
import { useApiClient } from '@broccoli/web-sdk/api';
import type { TestCaseSummary } from '@broccoli/web-sdk/problem';
import { useCallback, useEffect, useState } from 'react';

export function useTestCases(problemId: number | undefined) {
  const apiClient = useApiClient();
  const [testCases, setTestCases] = useState<TestCaseSummary[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchTestCases = useCallback(async () => {
    if (!problemId) return;
    setLoading(true);
    setError(null);
    try {
      const { data, error } = await apiClient.GET('/problems/{id}/test-cases', {
        params: { path: { id: problemId } },
      });
      if (error || !data) {
        throw new Error('Failed to fetch test cases');
      }
      setTestCases(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [apiClient, problemId]);

  useEffect(() => {
    fetchTestCases();
  }, [fetchTestCases]);

  return { testCases, loading, error, refetch: fetchTestCases };
}
