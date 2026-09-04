import { useApiFetch } from '@broccoli/web-sdk/api';
import { useQuery } from '@tanstack/react-query';

import type { DlqStats } from '@/features/dlq/types';

const REFETCH_INTERVAL_MS = 10_000;

export function useDlqStats({ enabled = true }: { enabled?: boolean } = {}) {
  const apiFetch = useApiFetch();

  return useQuery({
    queryKey: ['dlq', 'stats'],
    // `/dlq/stats` requires dlq:manage. Callers that render before their own
    // permission check must pass `enabled: false`, or this poll 403-loops for
    // unauthorized viewers (hooks run before an early Unauthorized return).
    enabled,
    refetchInterval: REFETCH_INTERVAL_MS,
    refetchIntervalInBackground: false,
    queryFn: async (): Promise<DlqStats> => {
      const res = await apiFetch('/api/v1/dlq/stats');
      if (!res.ok) throw new Error(`Failed to load DLQ stats (${res.status})`);
      return (await res.json()) as DlqStats;
    },
  });
}
