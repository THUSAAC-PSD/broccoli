import { useApiFetch } from '@broccoli/web-sdk/api';
import { useQuery } from '@tanstack/react-query';

import type { DlqListResponse, DlqResolvedFilter } from '@/features/dlq/types';

interface Params {
  page: number;
  perPage: number;
  resolvedFilter: DlqResolvedFilter;
  messageType?: string;
}

const REFETCH_INTERVAL_MS = 10_000;

export function useDlqList({
  page,
  perPage,
  resolvedFilter,
  messageType,
}: Params) {
  const apiFetch = useApiFetch();

  return useQuery({
    queryKey: ['dlq', 'list', { page, perPage, resolvedFilter, messageType }],
    refetchInterval: REFETCH_INTERVAL_MS,
    refetchIntervalInBackground: false,
    queryFn: async (): Promise<DlqListResponse> => {
      const search = new URLSearchParams();
      search.set('page', String(page));
      search.set('per_page', String(perPage));
      if (resolvedFilter !== 'all') {
        search.set(
          'resolved',
          resolvedFilter === 'resolved' ? 'true' : 'false',
        );
      }
      if (messageType) search.set('message_type', messageType);
      const res = await apiFetch(`/dlq?${search.toString()}`);
      if (!res.ok) throw new Error(`Failed to load DLQ list (${res.status})`);
      return (await res.json()) as DlqListResponse;
    },
  });
}
