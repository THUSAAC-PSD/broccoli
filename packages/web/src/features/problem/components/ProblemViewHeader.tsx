import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Slot } from '@broccoli/web-sdk/slot';
import { Button } from '@broccoli/web-sdk/ui';
import { formatKibibytes } from '@broccoli/web-sdk/utils';
import { Edit } from 'lucide-react';

import { useAuth } from '@/features/auth/hooks/use-auth';
import { ContestCountdownMini } from '@/features/contest/components/ContestCountdown';

import { ProblemHeader } from './ProblemHeader';

interface ProblemViewHeaderProps {
  problem:
    | {
        id: number;
        title: string;
        problem_type: string;
        time_limit: number;
        memory_limit: number;
      }
    | undefined;
  headerId: string;
  contestId?: number;
  onEdit: () => void;
}

export function ProblemViewHeader({
  problem,
  headerId,
  contestId,
  onEdit,
}: ProblemViewHeaderProps) {
  const { t } = useTranslation();
  const { user } = useAuth();

  const timeLimit = problem ? `${problem.time_limit} ms` : '—';
  const memoryLimit = problem ? formatKibibytes(problem.memory_limit) : '—';

  return (
    <div className="shrink-0 px-5 pt-4 pb-3">
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <ProblemHeader
            id={headerId}
            title={problem?.title ?? t('problem.title')}
            type={problem?.problem_type ?? '—'}
            io="Standard Input / Output"
            timeLimit={timeLimit}
            memoryLimit={memoryLimit}
          />
        </div>
        <div className="flex items-center gap-2 shrink-0">
          {contestId && (
            <div className="hidden lg:flex items-center">
              <ContestCountdownMini />
            </div>
          )}
          {user && user.permissions.includes('problem:edit') && (
            <Button
              onClick={onEdit}
              size="sm"
              variant="default"
              className="gap-1.5 h-8 px-3.5 text-xs font-medium"
            >
              <Edit className="h-3 w-3" />
              {t('problem.edit')}
            </Button>
          )}
        </div>
      </div>
      <Slot name="problem-detail.header" as="div" className="relative mt-2" />
    </div>
  );
}
