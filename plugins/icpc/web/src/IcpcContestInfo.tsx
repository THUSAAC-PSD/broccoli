import { Badge } from '@broccoli/web-sdk/ui';
import { useParams } from 'react-router';

import { useIsIcpcContest } from './hooks/useIsIcpcContest';

export function IcpcContestInfo() {
  const { contestId } = useParams();
  const cId = contestId ? Number(contestId) : undefined;
  const { isIcpc, contestInfo, isLoading } = useIsIcpcContest(cId);

  if (isLoading || !isIcpc || !contestInfo) return null;

  return (
    <div className="rounded-lg border border-border bg-card mb-4 p-4 text-left">
      <div className="flex items-center gap-2 mb-2">
        <Badge
          variant="default"
          className="uppercase text-[11px] font-bold tracking-wide"
        >
          ICPC
        </Badge>
        <span className="text-sm font-semibold text-foreground">
          ACM-ICPC Style
        </span>
      </div>

      <div className="text-xs text-muted-foreground mb-2.5">
        Ranked by problems solved, then total time penalty. Each wrong
        submission adds {contestInfo.penalty_minutes} minutes of penalty.
        {contestInfo.count_compile_error
          ? ' Compilation errors count as attempts.'
          : ' Compilation errors do not count as attempts.'}
      </div>

      <div className="flex flex-wrap gap-4 text-xs text-muted-foreground justify-start">
        <span className="inline-flex items-center gap-1 rounded bg-muted text-[11px] font-medium px-1.5 py-0.5">
          {contestInfo.penalty_minutes} min/attempt
        </span>
        {contestInfo.show_test_details && (
          <span className="inline-flex items-center gap-1 rounded bg-muted text-[11px] font-medium px-1.5 py-0.5">
            Test details visible
          </span>
        )}
      </div>
    </div>
  );
}
