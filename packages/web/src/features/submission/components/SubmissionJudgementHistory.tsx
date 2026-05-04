import { useApiClient } from '@broccoli/web-sdk/api';
import { useAuth } from '@broccoli/web-sdk/auth';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import type { SubmissionJudgement } from '@broccoli/web-sdk/submission';
import { Badge, Button } from '@broccoli/web-sdk/ui';
import { formatRelativeDatetime } from '@broccoli/web-sdk/utils';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Check, Loader2, RotateCcw, Trash2 } from 'lucide-react';
import { useMemo, useState } from 'react';
import { toast } from 'sonner';

import { useSystemOverview } from '@/features/system/hooks/useSystemOverview';
import { extractErrorMessage } from '@/lib/extract-error';

import { getVerdictBadge } from '../utils/verdict';

interface Props {
  submissionId: number;
}

export function SubmissionJudgementHistory({ submissionId }: Props) {
  const { t } = useTranslation();
  const { user } = useAuth();
  const apiClient = useApiClient();
  const queryClient = useQueryClient();
  const canViewHistory = !!user;
  const canRejudge = !!user?.permissions.includes('submission:rejudge');
  const canPinWorker = !!user?.permissions.includes('system:admin');
  const [targetWorkerId, setTargetWorkerId] = useState('');

  const { data: systemOverview } = useSystemOverview();
  const liveWorkers = useMemo(
    () => (systemOverview?.workers ?? []).filter((w) => !w.stale),
    [systemOverview],
  );

  const { data: judgements = [], isLoading } = useQuery({
    queryKey: ['submission-judgements', submissionId],
    enabled: canViewHistory,
    queryFn: async () => {
      const { data, error } = await apiClient.GET(
        '/submissions/{id}/judgements',
        {
          params: { path: { id: submissionId } },
        },
      );
      if (error) throw error;
      return data;
    },
  });

  const invalidate = async () => {
    await queryClient.invalidateQueries({
      queryKey: ['submission', submissionId],
    });
    await queryClient.invalidateQueries({
      queryKey: ['submission-judgements', submissionId],
    });
  };

  const rejudgeMutation = useMutation({
    mutationFn: async (applyImmediately: boolean) => {
      const body = {
        apply_immediately: applyImmediately,
        ...(canPinWorker && targetWorkerId
          ? { target_worker_id: targetWorkerId }
          : {}),
      };
      const { data, error } = await apiClient.POST(
        '/submissions/{id}/rejudge',
        {
          params: { path: { id: submissionId } },
          body,
        },
      );
      if (error) throw error;
      return data;
    },
    onSuccess: async () => {
      toast.success(t('submissionDetail.rejudgeQueued'));
      await invalidate();
    },
    onError: (error) => {
      toast.error(
        extractErrorMessage(error, t('submissionDetail.rejudgeError')),
      );
    },
  });

  const applyMutation = useMutation({
    mutationFn: async (judgementId: number) => {
      const { data, error } = await apiClient.POST(
        '/submissions/{id}/judgements/{judgement_id}/apply',
        {
          params: { path: { id: submissionId, judgement_id: judgementId } },
        },
      );
      if (error) throw error;
      return data;
    },
    onSuccess: async () => {
      toast.success(t('submissionDetail.judgementApplied'));
      await invalidate();
    },
    onError: (error) => {
      toast.error(extractErrorMessage(error, t('submissionDetail.applyError')));
    },
  });

  const discardMutation = useMutation({
    mutationFn: async (judgementId: number) => {
      const { error } = await apiClient.POST(
        '/submissions/{id}/judgements/{judgement_id}/discard',
        {
          params: { path: { id: submissionId, judgement_id: judgementId } },
        },
      );
      if (error) throw error;
    },
    onSuccess: async () => {
      toast.success(t('submissionDetail.judgementDiscarded'));
      await invalidate();
    },
    onError: (error) => {
      toast.error(
        extractErrorMessage(error, t('submissionDetail.discardError')),
      );
    },
  });

  if (!canViewHistory) return null;

  return (
    <div className="rounded-lg border bg-card px-6 py-5">
      <div className="flex flex-wrap items-center gap-3">
        <h2 className="text-sm font-semibold">
          {t('submissionDetail.versions')}
        </h2>
        {isLoading && (
          <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
        )}
        {canRejudge && (
          <>
            {canPinWorker && liveWorkers.length > 0 && (
              <select
                value={targetWorkerId}
                onChange={(event) => setTargetWorkerId(event.target.value)}
                className="ml-auto h-8 rounded-md border bg-background px-2 text-xs"
              >
                <option value="">
                  {t('submissionDetail.workerSharedPool')}
                </option>
                {liveWorkers.map((worker) => (
                  <option key={worker.id} value={worker.id}>
                    {worker.id}
                  </option>
                ))}
              </select>
            )}
            <Button
              size="sm"
              variant="outline"
              disabled={rejudgeMutation.isPending}
              onClick={() => rejudgeMutation.mutate(false)}
            >
              {rejudgeMutation.isPending ? (
                <Loader2 className="mr-1 h-4 w-4 animate-spin" />
              ) : (
                <RotateCcw className="mr-1 h-4 w-4" />
              )}
              {t('submissionDetail.regradeCandidate')}
            </Button>
            <Button
              size="sm"
              disabled={rejudgeMutation.isPending}
              onClick={() => rejudgeMutation.mutate(true)}
            >
              <RotateCcw className="mr-1 h-4 w-4" />
              {t('submissionDetail.rejudgeNow')}
            </Button>
          </>
        )}
      </div>

      <div className="mt-4 divide-y">
        {judgements.map((judgement) => (
          <JudgementRow
            key={judgement.id}
            judgement={judgement}
            canManage={canRejudge}
            applying={applyMutation.isPending}
            discarding={discardMutation.isPending}
            onApply={() => applyMutation.mutate(judgement.id)}
            onDiscard={() => discardMutation.mutate(judgement.id)}
          />
        ))}
      </div>
    </div>
  );
}

function JudgementRow({
  judgement,
  canManage,
  applying,
  discarding,
  onApply,
  onDiscard,
}: {
  judgement: SubmissionJudgement;
  canManage: boolean;
  applying: boolean;
  discarding: boolean;
  onApply: () => void;
  onDiscard: () => void;
}) {
  const { t } = useTranslation();
  const { label, variant } = getVerdictBadge(
    judgement.verdict ?? null,
    judgement.status,
    t,
  );

  return (
    <div className="flex flex-wrap items-center gap-3 py-3 text-sm">
      <div className="w-16 font-mono font-semibold tabular-nums">
        v{judgement.version}
      </div>
      <Badge variant={variant}>{label}</Badge>
      {judgement.is_current && (
        <Badge variant="outline">{t('submissionDetail.currentVersion')}</Badge>
      )}
      {!judgement.is_finalized && (
        <Badge variant="secondary">
          {t('submissionDetail.pendingVersion')}
        </Badge>
      )}
      {judgement.target_worker_id && (
        <Badge variant="outline" className="font-mono">
          {judgement.target_worker_id}
        </Badge>
      )}
      <span className="text-xs text-muted-foreground">
        {formatRelativeDatetime(judgement.created_at, t)}
      </span>
      {judgement.score != null && (
        <span className="ml-auto font-mono tabular-nums">
          {judgement.score} {t('result.pointsUnit')}
        </span>
      )}
      {canManage && !judgement.is_current && judgement.is_finalized && (
        <Button
          size="sm"
          variant="outline"
          disabled={applying}
          onClick={onApply}
        >
          <Check className="mr-1 h-4 w-4" />
          {t('submissionDetail.applyVersion')}
        </Button>
      )}
      {canManage && !judgement.is_current && judgement.is_finalized && (
        <Button
          size="sm"
          variant="ghost"
          disabled={discarding}
          onClick={onDiscard}
        >
          <Trash2 className="mr-1 h-4 w-4" />
          {t('submissionDetail.discardVersion')}
        </Button>
      )}
    </div>
  );
}
