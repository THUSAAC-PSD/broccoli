/**
 * Fetches test cases for a problem from the platform API.
 */
import type { TestCaseListItem } from '@broccoli/sdk';
import { useCallback, useEffect, useState } from 'react';

// Derive backend origin from where this module was loaded (same pattern as useIoiApi).
const BACKEND_ORIGIN = new URL(import.meta.url).origin;
const AUTH_TOKEN_KEY = 'broccoli_token';

export function useTestCases(problemId: number | undefined) {
  const [testCases, setTestCases] = useState<TestCaseListItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchTestCases = useCallback(async () => {
    if (!problemId) return;
    setLoading(true);
    setError(null);
    try {
      const token = localStorage.getItem(AUTH_TOKEN_KEY);
      const res = await fetch(
        `${BACKEND_ORIGIN}/api/v1/problems/${problemId}/test-cases`,
        {
          headers: token ? { Authorization: `Bearer ${token}` } : {},
        },
      );
      if (!res.ok) {
        const body = await res.json().catch(() => ({}));
        throw new Error(
          body.message || `Failed to fetch test cases (${res.status})`,
        );
      }
      const data: TestCaseListItem[] = await res.json();
      setTestCases(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [problemId]);

  useEffect(() => {
    fetchTestCases();
  }, [fetchTestCases]);

  return { testCases, loading, error, refetch: fetchTestCases };
}
