import type { ApiClient } from '@broccoli/web-sdk/api';

import type { ServerTableParams } from '@/hooks/use-server-table';

export async function fetchProblems(
  apiClient: ApiClient,
  params: ServerTableParams,
) {
  const { data, error } = await apiClient.GET('/problems', {
    params: {
      query: {
        page: params.page,
        per_page: params.per_page,
        search: params.search,
        sort_by: params.sort_by,
        sort_order: params.sort_order,
      },
    },
  });
  if (error) throw error;
  return { data: data.data, pagination: data.pagination };
}
