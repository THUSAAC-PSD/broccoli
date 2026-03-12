import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';

type ApiErrorLike = {
  code?: string;
};

type ContestEnrollState = {
  is_public: boolean;
  end_time: string;
  is_registered?: boolean;
};

export function useContestEnroll({
  contestId,
  contest,
  canManageContest,
}: {
  contestId: number;
  contest?: ContestEnrollState;
  canManageContest: boolean;
}) {
  const { t } = useTranslation();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();

  const enrollMutation = useMutation({
    mutationFn: async () => {
      const { error } = await apiClient.POST('/contests/{id}/register', {
        params: { path: { id: contestId } },
      });
      if (error) throw error;
    },
    onSuccess: () => {
      toast.success(t('toast.contest.enrolled'));
      queryClient.invalidateQueries({ queryKey: ['contest', contestId] });
      queryClient.invalidateQueries({ queryKey: ['dashboard-contests'] });
    },
    onError: (error: unknown) => {
      if ((error as ApiErrorLike)?.code === 'CONFLICT') {
        toast.success(t('toast.contest.enrolled'));
        queryClient.invalidateQueries({ queryKey: ['contest', contestId] });
        return;
      }
      toast.error(t('toast.contest.enrollError'));
    },
  });

  const isRegistered = Boolean(contest?.is_registered);
  const hasEnded = contest
    ? Date.now() >= new Date(contest.end_time).getTime()
    : true;

  const canShowEnrollCard =
    !!contest &&
    !canManageContest &&
    contest.is_public &&
    !isRegistered &&
    !hasEnded;

  return {
    canShowEnrollCard,
    enroll: () => enrollMutation.mutate(),
    isPending: enrollMutation.isPending,
  };
}
