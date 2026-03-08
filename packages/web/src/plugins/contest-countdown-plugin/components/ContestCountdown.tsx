import type { ContestResponse } from '@broccoli/web-sdk';
import { useApiClient } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useQuery } from '@tanstack/react-query';
import { Fragment, useEffect, useState } from 'react';
import { useParams } from 'react-router';

// ── helpers ──────────────────────────────────────────────────────────────────

function getTimeLeft(target: Date) {
  const ms = Math.max(0, target.getTime() - Date.now());
  const totalSecs = Math.floor(ms / 1000);
  return {
    days: Math.floor(totalSecs / 86400),
    hours: Math.floor((totalSecs % 86400) / 3600),
    minutes: Math.floor((totalSecs % 3600) / 60),
    seconds: totalSecs % 60,
    ms,
  };
}

// CSS-variable-based color tokens — respond to any theme automatically
const ACCENT = 'hsl(var(--sidebar-ring))';

// ── shared data hook ──────────────────────────────────────────────────────────

function useCountdownData(contestId: number) {
  const apiClient = useApiClient();
  const [tick, setTick] = useState(0);

  useEffect(() => {
    const timer = setInterval(() => setTick((n) => n + 1), 1000);
    return () => clearInterval(timer);
  }, []);

  const { data: contest } = useQuery({
    queryKey: ['contest', contestId],
    enabled: Number.isFinite(contestId) && contestId > 0,
    queryFn: async () => {
      const { data, error } = await apiClient.GET('/contests/{id}', {
        params: { path: { id: contestId } },
      });
      if (error) throw error;
      return data as ContestResponse;
    },
    staleTime: 60_000,
  });

  if (!contest) return null;

  const now = Date.now();
  const startMs = new Date(contest.start_time).getTime();
  const endMs = new Date(contest.end_time).getTime();

  const phase: 'upcoming' | 'running' | 'ended' =
    now < startMs ? 'upcoming' : now <= endMs ? 'running' : 'ended';

  const target =
    phase === 'upcoming'
      ? new Date(contest.start_time)
      : new Date(contest.end_time);

  const tl = getTimeLeft(target);
  const totalDuration = endMs - startMs;
  const elapsed = Math.max(0, now - startMs);
  const progress =
    phase === 'running' ? Math.min(100, (elapsed / totalDuration) * 100) : 0;

  // suppress unused tick warning — it drives re-renders
  void tick;

  return { contest, phase, tl, progress };
}

// ── full card ─────────────────────────────────────────────────────────────────

export function ContestCountdown() {
  const { contestId } = useParams();
  const { t, locale } = useTranslation();
  const d = useCountdownData(Number(contestId));

  if (!d) return null;
  const { contest, phase, tl, progress } = d;

  const active = phase !== 'ended';
  const segments =
    tl.days > 0
      ? [
          { value: tl.days, label: t('countdown.days') },
          { value: tl.hours, label: t('countdown.hours') },
          { value: tl.minutes, label: t('countdown.minutes') },
          { value: tl.seconds, label: t('countdown.seconds') },
        ]
      : [
          { value: tl.hours, label: t('countdown.hours') },
          { value: tl.minutes, label: t('countdown.minutes') },
          { value: tl.seconds, label: t('countdown.seconds') },
        ];

  const phaseLabel =
    phase === 'upcoming'
      ? t('countdown.startsIn')
      : phase === 'running'
        ? t('countdown.endsIn')
        : t('countdown.contestOver');

  const fmtDate = (dateStr: string) =>
    new Date(dateStr).toLocaleDateString(locale, {
      month: 'short',
      day: 'numeric',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });

  return (
    <div className="flex-1 flex items-center min-w-0">
      {!active ? (
        <div className="flex items-center gap-1.5">
          <span className="text-[10px] font-semibold uppercase tracking-[0.15em]">
            {phaseLabel}
          </span>
          <p className="text-sm text-muted-foreground">
            {t('countdown.finishedMessage')}
          </p>
        </div>
      ) : (
        <>
          {/* Center: phase label + big digits */}
          <div className="flex-1 flex flex-col items-center min-w-0">
            <div className="flex items-center gap-1.5 mb-1">
              <span
                className={`inline-block h-1.5 w-1.5 rounded-full ${phase === 'running' ? 'animate-pulse' : ''}`}
                style={{ backgroundColor: ACCENT }}
              />
              <span
                className="text-[10px] font-semibold uppercase tracking-[0.15em]"
                style={{ color: ACCENT }}
              >
                {phaseLabel}
              </span>
            </div>
            <div className="flex items-end gap-2">
              {segments.map((seg, i) => (
                <Fragment key={seg.label}>
                  <div className="flex flex-col items-center">
                    <span
                      className="tabular-nums text-4xl font-bold leading-none tracking-tighter"
                      style={{ color: ACCENT }}
                    >
                      {String(seg.value).padStart(2, '0')}
                    </span>
                    <span className="mt-0.5 text-[8px] font-medium uppercase tracking-widest text-muted-foreground">
                      {seg.label}
                    </span>
                  </div>
                  {i < segments.length - 1 && (
                    <span
                      className="mb-4 select-none text-xl font-extralight opacity-30"
                      style={{ color: ACCENT }}
                    >
                      :
                    </span>
                  )}
                </Fragment>
              ))}
            </div>
          </div>

          {/* Right: schedule + progress */}
          <div className="hidden sm:flex flex-col items-end gap-2 shrink-0">
            <div className="text-[11px] text-muted-foreground/70 whitespace-nowrap font-medium">
              {fmtDate(contest.start_time)} → {fmtDate(contest.end_time)}
            </div>
            {phase === 'running' && (
              <div className="w-full">
                <div className="w-full h-1 overflow-hidden rounded-full bg-foreground/5">
                  <div
                    className="h-full rounded-full transition-none"
                    style={{
                      width: `${progress}%`,
                      backgroundColor: ACCENT,
                    }}
                  />
                </div>
                <div className="mt-0.5 text-right text-[9px] tabular-nums text-muted-foreground/50">
                  {Math.round(progress)}%
                </div>
              </div>
            )}
          </div>
        </>
      )}
    </div>
  );
}

// ── mini (problem-detail top-right) ──────────────────────────────────────────

export function ContestCountdownMini() {
  const { contestId } = useParams();
  const { t } = useTranslation();
  const d = useCountdownData(Number(contestId));

  // Only render inside a contest route with live data
  if (!contestId || !d || d.phase === 'ended') return null;
  const { phase, tl } = d;

  const segments =
    tl.days > 0
      ? [
          { value: tl.days, label: t('countdown.days') },
          { value: tl.hours, label: t('countdown.hours') },
          { value: tl.minutes, label: t('countdown.minutes') },
          { value: tl.seconds, label: t('countdown.seconds') },
        ]
      : [
          { value: tl.hours, label: t('countdown.hours') },
          { value: tl.minutes, label: t('countdown.minutes') },
          { value: tl.seconds, label: t('countdown.seconds') },
        ];

  const phaseLabel =
    phase === 'upcoming' ? t('countdown.startsIn') : t('countdown.endsIn');

  return (
    // hidden on narrow viewports; vertically centered, right margin = px-6
    <div className="hidden lg:flex absolute right-6 top-1/2 -translate-y-1/2">
      <div className="inline-flex items-center gap-3 px-2" style={{}}>
        {/* dot */}
        <span
          className={`h-2 w-2 shrink-0 rounded-full ${phase === 'running' ? 'animate-pulse' : ''}`}
          style={{ backgroundColor: ACCENT }}
        />
        {/* label */}
        <span
          className="text-sm font-semibold uppercase tracking-widest"
          style={{ color: ACCENT }}
        >
          {phaseLabel}
        </span>
        {/* digits */}
        <div className="flex items-center gap-2">
          {segments.map((seg, i) => (
            <Fragment key={seg.label}>
              <div className="flex flex-col items-center leading-none gap-1">
                <span
                  className="tabular-nums text-4xl font-bold tracking-tighter"
                  style={{ color: ACCENT }}
                >
                  {String(seg.value).padStart(2, '0')}
                </span>
                <span className="text-xs uppercase tracking-wider text-muted-foreground/60">
                  {seg.label}
                </span>
              </div>
              {i < segments.length - 1 && (
                <span
                  className="mb-5 select-none text-2xl font-light opacity-25"
                  style={{ color: ACCENT }}
                >
                  :
                </span>
              )}
            </Fragment>
          ))}
        </div>
      </div>
    </div>
  );
}
