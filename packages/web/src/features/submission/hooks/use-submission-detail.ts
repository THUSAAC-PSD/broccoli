import { useApiClient } from '@broccoli/web-sdk/api';
import {
  isTerminalStatus,
  type Submission,
} from '@broccoli/web-sdk/submission';
import { useQuery } from '@tanstack/react-query';

export function useSubmissionDetail(submissionId: number) {
  const apiClient = useApiClient();

  const {
    data: submission,
    isLoading,
    error,
  } = useQuery<Submission>({
    queryKey: ['submission', submissionId],
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/submissions/{id}', {
        params: { path: { id: submissionId } },
      });
      if (error) throw error;
      return data;
    },
    refetchInterval: (query) => {
      const status = query.state.data?.status;
      if (status && isTerminalStatus(status)) return false;
      return 1000;
    },
  });

  return { submission: submission ?? null, isLoading, error };
}
