import { useTranslation } from '@broccoli/web-sdk/i18n';
import { Badge } from '@broccoli/web-sdk/ui';
import { AlertTriangle, CircleDot, Cpu } from 'lucide-react';

import type { WorkerInfo } from '@/features/system/types';

interface Props {
  worker: WorkerInfo;
}

function formatRelative(seconds: number, t: (k: string, p?: object) => string) {
  if (seconds < 5) return t('system.worker.justNow');
  if (seconds < 60) return t('system.worker.secondsAgo', { count: seconds });
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return t('system.worker.minutesAgo', { count: minutes });
  const hours = Math.floor(minutes / 60);
  return t('system.worker.hoursAgo', { count: hours });
}

function formatUptime(startedAt: string, t: (k: string, p?: object) => string) {
  const seconds = Math.max(
    0,
    Math.floor((Date.now() - new Date(startedAt).getTime()) / 1000),
  );
  if (seconds < 60) return t('system.worker.secondsUptime', { count: seconds });
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return t('system.worker.minutesUptime', { count: minutes });
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return t('system.worker.hoursUptime', { count: hours });
  const days = Math.floor(hours / 24);
  return t('system.worker.daysUptime', { count: days });
}

export function WorkerCard({ worker }: Props) {
  const { t } = useTranslation();

  const inFlightPct = worker.max_concurrency
    ? Math.min(100, (worker.in_flight / worker.max_concurrency) * 100)
    : worker.in_flight > 0
      ? 100
      : 0;

  return (
    <div className="rounded-lg border bg-card p-4 transition-colors hover:bg-accent/30">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="font-mono text-sm font-medium truncate">
              {worker.id}
            </span>
            {worker.stale ? (
              <Badge
                variant="secondary"
                className="gap-1 text-amber-600 dark:text-amber-400"
              >
                <AlertTriangle className="h-3 w-3" />
                {t('system.worker.stale')}
              </Badge>
            ) : (
              <Badge variant="outline" className="gap-1 border-emerald-500/40">
                <CircleDot className="h-3 w-3 text-emerald-500" />
                {t('system.worker.online')}
              </Badge>
            )}
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
            <span className="inline-flex items-center gap-1">
              <Cpu className="h-3 w-3" />
              {worker.sandbox_backend}
            </span>
            <span aria-hidden>·</span>
            <span>v{worker.version}</span>
            <span aria-hidden>·</span>
            <span>
              {t('system.worker.uptime', {
                time: formatUptime(worker.started_at, t),
              })}
            </span>
          </div>
        </div>
        <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
          {formatRelative(worker.seconds_since_last_seen, t)}
        </span>
      </div>

      <div className="mt-4">
        <div className="flex items-center justify-between text-xs">
          <span className="text-muted-foreground">
            {t('system.worker.inFlight')}
          </span>
          <span className="font-medium tabular-nums">
            {worker.in_flight}
            {worker.max_concurrency ? ` / ${worker.max_concurrency}` : ''}
          </span>
        </div>
        <div className="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-muted">
          <div
            className={`h-full rounded-full transition-all ${
              worker.in_flight > 0 ? 'bg-primary' : 'bg-muted'
            }`}
            style={{ width: `${inFlightPct}%` }}
          />
        </div>
      </div>
    </div>
  );
}
