import type { ApiClient } from '@broccoli/web-sdk/api';
import type {
  ServerTableParams,
  ServerTableResponse,
} from '@broccoli/web-sdk/hooks';
import type {
  SubmissionStatus,
  SubmissionSummary,
} from '@broccoli/web-sdk/submission';

export async function fetchContestSubmissions(
  apiClient: ApiClient,
  params: ServerTableParams & {
    contestId: number;
    problemId?: number | null;
    language?: string | null;
    status?: SubmissionStatus | null;
    userId?: number | undefined;
  },
): Promise<ServerTableResponse<SubmissionSummary>> {
  const { data, error } = await apiClient.GET('/contests/{id}/submissions', {
    params: {
      path: { id: params.contestId },
      query: {
        page: params.page,
        per_page: params.per_page,
        sort_by: params.sort_by,
        sort_order: params.sort_order,
        user_id: params.userId,
        ...(params.problemId ? { problem_id: params.problemId } : {}),
        ...(params.language ? { language: params.language } : {}),
        ...(params.status ? { status: params.status } : {}),
      },
    },
  });

  if (error) throw error;

  return {
    data: data.data,
    pagination: data.pagination,
  };
}
