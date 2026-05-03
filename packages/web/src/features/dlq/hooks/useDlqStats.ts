import { useApiFetch } from '@broccoli/web-sdk/api';
import { useQuery } from '@tanstack/react-query';

import type { DlqStats } from '@/features/dlq/types';

const REFETCH_INTERVAL_MS = 10_000;

export function useDlqStats() {
  const apiFetch = useApiFetch();

  return useQuery({
    queryKey: ['dlq', 'stats'],
    refetchInterval: REFETCH_INTERVAL_MS,
    refetchIntervalInBackground: false,
    queryFn: async (): Promise<DlqStats> => {
      const res = await apiFetch('/dlq/stats');
      if (!res.ok) throw new Error(`Failed to load DLQ stats (${res.status})`);
      return (await res.json()) as DlqStats;
    },
  });
}
