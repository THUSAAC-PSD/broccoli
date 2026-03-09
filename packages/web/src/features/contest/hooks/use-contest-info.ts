import { useApiClient } from '@broccoli/web-sdk/api';
import { useQuery } from '@tanstack/react-query';

export function useContestInfo(contestId: number) {
  const apiClient = useApiClient();
  const {
    data: contest,
    isLoading,
    error,
  } = useQuery({
    queryKey: ['contest', contestId],
    enabled: Number.isFinite(contestId),
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/contests/{id}', {
        params: { path: { id: contestId } },
      });
      if (error) throw error;
      return data;
    },
  });
  return { contest, isLoading, error };
}
