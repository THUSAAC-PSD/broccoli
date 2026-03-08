import type { SubmissionListItem, SubmissionStatus } from '@broccoli/web-sdk';
import type { ApiClient } from '@broccoli/web-sdk/api';

import type {
  ServerTableParams,
  ServerTableResponse,
} from '@/hooks/use-server-table';

export async function fetchContestSubmissions(
  apiClient: ApiClient,
  params: ServerTableParams & {
    contestId: number;
    problemId?: number | null;
    language?: string | null;
    status?: SubmissionStatus | null;
  },
): Promise<ServerTableResponse<SubmissionListItem>> {
  const { data, error } = await apiClient.GET('/contests/{id}/submissions', {
    params: {
      path: { id: params.contestId },
      query: {
        page: params.page,
        per_page: params.per_page,
        sort_by: params.sort_by,
        sort_order: params.sort_order,
        ...(params.problemId ? { problem_id: params.problemId } : {}),
        ...(params.language ? { language: params.language } : {}),
        ...(params.status ? { status: params.status } : {}),
      },
    },
  });

  if (error) throw error;

  return {
    data: data.data as SubmissionListItem[],
    pagination: data.pagination,
  };
}
