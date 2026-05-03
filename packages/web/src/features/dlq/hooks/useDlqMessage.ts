import { useApiFetch } from '@broccoli/web-sdk/api';
import { useQuery } from '@tanstack/react-query';

import type { DlqMessageDetail } from '@/features/dlq/types';

export function useDlqMessage(id: number | null) {
  const apiFetch = useApiFetch();

  return useQuery({
    queryKey: ['dlq', 'message', id],
    enabled: id !== null,
    queryFn: async (): Promise<DlqMessageDetail> => {
      const res = await apiFetch(`/api/v1/dlq/${id}`);
      if (!res.ok)
        throw new Error(`Failed to load DLQ message (${res.status})`);
      return (await res.json()) as DlqMessageDetail;
    },
  });
}
