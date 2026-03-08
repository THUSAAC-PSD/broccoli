import type { ContestProblemResponse } from '@broccoli/web-sdk';
import type { ApiClient } from '@broccoli/web-sdk/api';

export async function fetchContestProblemList(
  apiClient: ApiClient,
  contestId: number,
): Promise<ContestProblemResponse[]> {
  const { data, error } = await apiClient.GET('/contests/{id}/problems', {
    params: { path: { id: contestId } },
  });

  if (error) throw error;

  return data as ContestProblemResponse[];
}
