import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import {
  createClarification,
  fetchClarifications,
  replyClarification,
} from '../api/api';
import type { CreateClarificationBody } from '../api/types';

export function useClarifications(contestId: number, enabled: boolean) {
  return useQuery({
    queryKey: ['contest-clarifications', contestId],
    queryFn: () => fetchClarifications(contestId),
    enabled,
  });
}

export function useCreateClarification(contestId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateClarificationBody) =>
      createClarification(contestId, body),
    onSettled: () => {
      queryClient.invalidateQueries({
        queryKey: ['contest-clarifications', contestId],
      });
    },
  });
}

export function useReplyClarification(contestId: number) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (payload: {
      clarificationId: number;
      content: string;
      is_public: boolean;
    }) =>
      replyClarification(contestId, payload.clarificationId, {
        content: payload.content,
        is_public: payload.is_public,
      }),
    onSettled: () => {
      queryClient.invalidateQueries({
        queryKey: ['contest-clarifications', contestId],
      });
    },
  });
}
