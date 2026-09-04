import { useTranslation } from '@broccoli/web-sdk/i18n';
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@broccoli/web-sdk/ui';
import { AlertTriangle, Check, Clock, Loader2, WifiOff } from 'lucide-react';

import { useContestPrewarm } from '@/features/contest/hooks/use-contest-prewarm';

interface WorkerStatus {
  worker_id: string;
  warmed: number;
  total: number;
  fraction: number;
  state: string;
  stale: boolean;
  reported: boolean;
  error?: string | null;
}

type Tone = 'complete' | 'warming' | 'pending' | 'error';

function toneFor(state: string): Tone {
  if (state === 'complete' || state === 'warming' || state === 'error') {
    return state;
  }
  return 'pending';
}

export function PrewarmDialog({
  contestId,
  open,
  onOpenChange,
}: {
  contestId: number;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const { start, isStarting, status } = useContestPrewarm(contestId, open);

  const workers = (status?.workers ?? []) as WorkerStatus[];
  const workersTotal = status?.workers_total ?? 0;
  const workersReady = status?.workers_complete ?? 0;
  const totalBlobs = status?.total_blobs ?? 0;
  const overall = status?.overall_fraction ?? 0;

  const anyInFlight =
    isStarting ||
    workers.some((w) => w.state === 'warming' || w.state === 'pending');
  const hasStarted = workers.length > 0 || status?.job_id != null;
  const noJudges =
    status !== undefined && workersTotal === 0 && workers.length === 0;
  const allReady = workersTotal > 0 && workersReady === workersTotal;

  const startLabel = anyInFlight
    ? t('warm.warming')
    : hasStarted
      ? t('warm.rewarm')
      : t('warm.start');

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('warm.title')}</DialogTitle>
          <DialogDescription>{t('warm.description')}</DialogDescription>
        </DialogHeader>

        {/* Overall fleet readiness */}
        <div className="space-y-2">
          <div className="flex items-center justify-between text-xs">
            <span className="text-muted-foreground">
              {t('warm.testFiles', { count: totalBlobs })}
            </span>
            <span className="font-medium text-foreground tabular-nums">
              {t('warm.judgesReady', {
                ready: workersReady,
                total: workersTotal,
              })}
            </span>
          </div>
          <Track fraction={overall} tone={allReady ? 'complete' : 'warming'} />
        </div>

        {/* Per-judge rows */}
        <div className="mt-1 max-h-64 space-y-2.5 overflow-y-auto">
          {noJudges && (
            <p className="py-6 text-center text-sm text-muted-foreground">
              {t('warm.noJudges')}
            </p>
          )}
          {!hasStarted && !noJudges && workers.length === 0 && (
            <p className="py-6 text-center text-sm text-muted-foreground">
              {t('warm.notStarted')}
            </p>
          )}
          {workers.map((w) => (
            <JudgeRow key={w.worker_id} worker={w} />
          ))}
        </div>

        <DialogFooter>
          <Button onClick={start} disabled={anyInFlight} className="gap-2">
            {anyInFlight && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
            {startLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function JudgeRow({ worker }: { worker: WorkerStatus }) {
  return (
    <div className="flex items-center gap-3">
      <span
        className="w-28 shrink-0 truncate font-mono text-xs text-foreground"
        title={worker.worker_id}
      >
        {worker.worker_id}
      </span>
      <div className="flex-1">
        <Track fraction={worker.fraction} tone={toneFor(worker.state)} />
      </div>
      <span className="w-12 shrink-0 text-right text-xs tabular-nums text-muted-foreground">
        {worker.warmed}/{worker.total}
      </span>
      <StatePill worker={worker} />
    </div>
  );
}

function Track({ fraction, tone }: { fraction: number; tone: Tone }) {
  const pct = Math.round(Math.max(0, Math.min(1, fraction)) * 100);
  const bar: Record<Tone, string> = {
    complete: 'bg-emerald-500',
    warming: 'bg-primary',
    pending: 'bg-muted-foreground/30',
    error: 'bg-destructive',
  };
  return (
    <div className="h-1.5 w-full overflow-hidden rounded-full bg-muted">
      <div
        className={`h-full rounded-full transition-all duration-500 ease-out ${bar[tone]}`}
        style={{ width: `${pct}%` }}
      />
    </div>
  );
}

function StatePill({ worker }: { worker: WorkerStatus }) {
  const { t } = useTranslation();
  const base =
    'inline-flex w-20 shrink-0 items-center justify-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-medium';

  if (worker.stale) {
    return (
      <span className={`${base} bg-muted text-muted-foreground`}>
        <WifiOff className="h-3 w-3" />
        {t('warm.stale')}
      </span>
    );
  }

  switch (worker.state) {
    case 'complete':
      return (
        <span
          className={`${base} bg-emerald-500/10 text-emerald-600 dark:text-emerald-400`}
        >
          <Check className="h-3 w-3" />
          {t('warm.stateReady')}
        </span>
      );
    case 'warming':
      return (
        <span className={`${base} bg-primary/10 text-primary`}>
          <Loader2 className="h-3 w-3 animate-spin" />
          {t('warm.stateWarming')}
        </span>
      );
    case 'error':
      return (
        <span
          className={`${base} bg-destructive/10 text-destructive`}
          title={worker.error ?? undefined}
        >
          <AlertTriangle className="h-3 w-3" />
          {t('warm.stateError')}
        </span>
      );
    default:
      return (
        <span className={`${base} bg-muted text-muted-foreground`}>
          <Clock className="h-3 w-3" />
          {t('warm.statePending')}
        </span>
      );
  }
}
