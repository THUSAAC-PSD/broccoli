import type { ApiClient } from '@broccoli/web-sdk/api';
import type { ContestProblem } from '@broccoli/web-sdk/contest';

export async function fetchContestProblemList(
  apiClient: ApiClient,
  contestId: number,
): Promise<ContestProblem[]> {
  const { data, error } = await apiClient.GET('/contests/{id}/problems', {
    params: { path: { id: contestId } },
  });

  if (error) throw error;

  return data as ContestProblem[];
}
