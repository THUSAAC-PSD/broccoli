import { useApiClient, useApiFetch } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';

import {
  createClarification,
  fetchClarifications,
  replyClarification,
  resolveClarification,
  toggleReplyPublic,
} from '../api/api';
import type { CreateClarificationBody } from '../api/types';

export function useClarifications(contestId: number, enabled: boolean) {
  const apiClient = useApiClient();
  return useQuery({
    queryKey: ['contest-clarifications', contestId],
    queryFn: () => fetchClarifications(apiClient, contestId),
    enabled,
    refetchInterval: 5000,
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
    mutationFn: (payload: { clarificationId: number; content: string }) =>
      replyClarification(apiClient, contestId, payload.clarificationId, {
        content: payload.content,
        is_public: false,
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

export function useResolveClarification(contestId: number) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (payload: { clarificationId: number; resolved: boolean }) =>
      resolveClarification(apiClient, contestId, payload.clarificationId, {
        resolved: payload.resolved,
      }),
    onSuccess: (_data, variables) => {
      toast.success(
        variables.resolved
          ? t('clarification.resolved')
          : t('clarification.reopened'),
      );
    },
    onError: (error: Error) => {
      toast.error(error.message || t('clarification.resolveError'));
    },
    onSettled: () => {
      queryClient.invalidateQueries({
        queryKey: ['contest-clarifications', contestId],
      });
    },
  });
}

export function useToggleReplyPublic(contestId: number) {
  const { t } = useTranslation();
  const apiFetch = useApiFetch();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (payload: {
      clarificationId: number;
      replyId: number;
      includeQuestion?: boolean;
    }) =>
      toggleReplyPublic(
        apiFetch,
        contestId,
        payload.clarificationId,
        payload.replyId,
        payload.includeQuestion,
      ),
    onSuccess: (data) => {
      toast.success(
        data.is_public
          ? t('clarification.madePublic')
          : t('clarification.madePrivate'),
      );
    },
    onError: (error: Error) => {
      toast.error(error.message || t('clarification.toggleError'));
    },
    onSettled: () => {
      queryClient.invalidateQueries({
        queryKey: ['contest-clarifications', contestId],
      });
    },
  });
}
