import type { ApiClient } from '@broccoli/web-sdk/api';
import type { ContestProblem } from '@broccoli/web-sdk/contest';
import type { ServerTableParams } from '@broccoli/web-sdk/hooks';
import type { ProblemSummary } from '@broccoli/web-sdk/problem';

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

  const normalizedData = data.map((p: ContestProblem) => ({
    ...p,
    id: p.problem_id,
    title: p.problem_title,
  })) as unknown as ProblemSummary[];

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
