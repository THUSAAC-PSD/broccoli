import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Slot } from '@broccoli/web-sdk/slot';
import { formatKibibytes } from '@broccoli/web-sdk/utils';

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
}

export function ProblemViewHeader({
  problem,
  headerId,
  contestId,
}: ProblemViewHeaderProps) {
  const { t } = useTranslation();

  const timeLimit = problem ? `${problem.time_limit} ms` : '—';
  const memoryLimit = problem ? formatKibibytes(problem.memory_limit) : '—';

  return (
    <div className="flex-shrink-0 px-6 pt-3 pb-2">
      <div className="flex items-start sm:items-center gap-4">
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
        <div className="flex items-center gap-2">
          {contestId && (
            <div className="hidden lg:flex items-center">
              <ContestCountdownMini />
            </div>
          )}
        </div>
      </div>
      <Slot name="problem-detail.header" as="div" className="relative mt-3" />
    </div>
  );
}
