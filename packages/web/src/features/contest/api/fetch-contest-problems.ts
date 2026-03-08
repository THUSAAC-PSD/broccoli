import type {
  ContestProblemResponse,
  ProblemListItem,
} from '@broccoli/web-sdk';
import type { ApiClient } from '@broccoli/web-sdk/api';

import type { ServerTableParams } from '@/hooks/use-server-table';

export async function fetchContestProblems(
  apiClient: ApiClient,
  params: ServerTableParams & { contestId: number },
) {
  const { data, error } = await apiClient.GET('/contests/{id}/problems', {
    params: {
      path: { id: params.contestId },
    },
  });

  if (error) throw error;

  const normalizedData = data.map((p: ContestProblemResponse) => ({
    ...p,
    id: p.problem_id,
    title: p.problem_title,
  })) as unknown as ProblemListItem[];

  return {
    data: normalizedData,
    pagination: {
      page: 1,
      per_page: normalizedData.length,
      total: normalizedData.length,
      total_pages: 1,
    },
  };
}
