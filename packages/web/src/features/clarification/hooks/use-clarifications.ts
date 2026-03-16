import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';

import {
  createClarification,
  fetchClarifications,
  replyClarification,
} from '../api/api';
import type { CreateClarificationBody } from '../api/types';

export function useClarifications(contestId: number, enabled: boolean) {
  const apiClient = useApiClient();
  return useQuery({
    queryKey: ['contest-clarifications', contestId],
    queryFn: () => fetchClarifications(apiClient, contestId),
    enabled,
  });
}

export function useCreateClarification(contestId: number) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateClarificationBody) =>
      createClarification(apiClient, contestId, body),
    onSuccess: () => {
      toast.success(t('clarification.submitSuccess'));
    },
    onError: (error: Error) => {
      toast.error(error.message || t('clarification.submitError'));
    },
    onSettled: () => {
      queryClient.invalidateQueries({
        queryKey: ['contest-clarifications', contestId],
      });
    },
  });
}

export function useReplyClarification(contestId: number) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (payload: {
      clarificationId: number;
      content: string;
      is_public: boolean;
    }) =>
      replyClarification(apiClient, contestId, payload.clarificationId, {
        content: payload.content,
        is_public: payload.is_public,
      }),
    onSuccess: () => {
      toast.success(t('clarification.replySuccess'));
    },
    onError: (error: Error) => {
      toast.error(error.message || t('clarification.replyError'));
    },
    onSettled: () => {
      queryClient.invalidateQueries({
        queryKey: ['contest-clarifications', contestId],
      });
    },
  });
}
