import { useApiFetch } from '@broccoli/web-sdk/api';
import { useQuery } from '@tanstack/react-query';

import type { SystemOverviewResponse } from '@/features/system/types';

const REFETCH_INTERVAL_MS = 5000;

export function useSystemOverview() {
  const apiFetch = useApiFetch();

  return useQuery({
    queryKey: ['system', 'overview'],
    refetchInterval: REFETCH_INTERVAL_MS,
    refetchIntervalInBackground: false,
    queryFn: async (): Promise<SystemOverviewResponse> => {
      const res = await apiFetch('/admin/system/overview');
      if (!res.ok) {
        throw new Error(`Failed to fetch system overview (${res.status})`);
      }
      return (await res.json()) as SystemOverviewResponse;
    },
  });
}
