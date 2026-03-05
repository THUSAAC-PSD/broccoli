import type { ContestResponse } from '@broccoli/sdk';
import { useApiClient } from '@broccoli/sdk/api';
import { useTranslation } from '@broccoli/sdk/i18n';
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
const ACCENT_15 = 'hsl(var(--sidebar-ring) / 0.15)';
const ACCENT_20 = 'hsl(var(--sidebar-ring) / 0.20)';
const ACCENT_06 = 'hsl(var(--sidebar-ring) / 0.06)';

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
  const { t } = useTranslation();
  const d = useCountdownData(Number(contestId));

  if (!d) return null;
  const { phase, tl, progress } = d;

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

  return (
    <div
      className="rounded-xl border px-6 py-6 transition-colors duration-500"
      style={
        active
          ? {
              borderColor: phase === 'upcoming' ? ACCENT_15 : ACCENT_20,
              background: `linear-gradient(to bottom, ${ACCENT_06}, transparent)`,
            }
          : undefined
      }
    >
      <div className="flex flex-col items-center gap-3">
        {/* Status label */}
        <div className="flex items-center gap-1.5">
          {active && (
            <span
              className={`inline-block h-1.5 w-1.5 rounded-full ${phase === 'running' ? 'animate-pulse' : ''}`}
              style={{ backgroundColor: ACCENT }}
            />
          )}
          <span
            className="text-[11px] font-semibold uppercase tracking-[0.2em]"
            style={active ? { color: ACCENT } : undefined}
          >
            {phaseLabel}
          </span>
        </div>

        {!active ? (
          <p className="py-2 text-base font-medium text-muted-foreground">
            {t('countdown.finishedMessage')}
          </p>
        ) : (
          <>
            {/* Digits */}
            <div className="flex items-end gap-2 sm:gap-3">
              {segments.map((seg, i) => (
                <Fragment key={seg.label}>
                  <div className="flex flex-col items-center gap-2">
                    <span
                      className="tabular-nums text-5xl font-bold leading-none tracking-tighter sm:text-6xl"
                      style={{ color: ACCENT }}
                    >
                      {String(seg.value).padStart(2, '0')}
                    </span>
                    <span className="text-[10px] font-medium uppercase tracking-widest text-muted-foreground">
                      {seg.label}
                    </span>
                  </div>
                  {i < segments.length - 1 && (
                    <span
                      className="mb-5 select-none text-2xl font-extralight opacity-30"
                      style={{ color: ACCENT }}
                    >
                      :
                    </span>
                  )}
                </Fragment>
              ))}
            </div>

            {/* Progress bar — running only */}
            {phase === 'running' && (
              <div className="mt-1 w-full max-w-[260px]">
                <div className="h-px w-full overflow-hidden rounded-full bg-foreground/10">
                  <div
                    className="h-full rounded-full transition-none"
                    style={{
                      width: `${progress}%`,
                      backgroundColor: ACCENT,
                    }}
                  />
                </div>
                <div className="mt-1.5 flex justify-between text-[9px] text-muted-foreground/50">
                  <span>{t('countdown.elapsed')}</span>
                  <span>{Math.round(progress)}%</span>
                </div>
              </div>
            )}
          </>
        )}
      </div>
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
