/**
 * Shows cooldown timer status on the problem detail sidebar.
 */
import { useApiFetch } from '@broccoli/web-sdk/api';
import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useCallback, useEffect, useRef, useState } from 'react';

interface Props {
  submission?: { id: number; status: string } | null;
  contestId?: number;
  problemId?: number;
}

interface CooldownStatusData {
  cooldown_seconds: number;
  seconds_since_last: number | null;
  can_submit: boolean;
}

const PLUGIN_BASE = '/api/v1/p/cooldown/api/plugins/cooldown';

const MONO: React.CSSProperties = {
  fontVariantNumeric: 'tabular-nums',
  fontFamily:
    'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
};

export function CooldownStatus({ submission, contestId, problemId }: Props) {
  const apiFetch = useApiFetch();
  const { t } = useTranslation();
  const [data, setData] = useState<CooldownStatusData | null>(null);
  const [remaining, setRemaining] = useState<number>(0);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const submissionId = submission?.id;

  const fetchStatus = useCallback(async () => {
    if (!contestId || !problemId) return;
    try {
      const res = await apiFetch(
        `${PLUGIN_BASE}/contests/${contestId}/problems/${problemId}/status`,
      );
      if (!res.ok) return;
      const d: CooldownStatusData = await res.json();
      setData(d);

      // Compute initial remaining seconds
      if (d.cooldown_seconds === 0 || d.can_submit) {
        setRemaining(0);
      } else if (d.seconds_since_last !== null) {
        setRemaining(
          Math.max(0, d.cooldown_seconds - Math.max(0, d.seconds_since_last)),
        );
      } else {
        setRemaining(0);
      }
    } catch {
      // silent
    }
  }, [apiFetch, contestId, problemId]);

  // Fetch on mount and when submission changes
  useEffect(() => {
    if (!contestId || !problemId) return;

    let cancelled = false;

    (async () => {
      await fetchStatus();
      if (cancelled) return;
    })();

    return () => {
      cancelled = true;
    };
  }, [contestId, problemId, submissionId, fetchStatus]);

  useEffect(() => {
    if (timerRef.current) clearInterval(timerRef.current);

    if (remaining > 0) {
      timerRef.current = setInterval(() => {
        setRemaining((prev) => {
          if (prev <= 1) {
            if (timerRef.current) clearInterval(timerRef.current);
            fetchStatus();
            return 0;
          }
          return prev - 1;
        });
      }, 1000);
    }

    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [fetchStatus, remaining > 0]);

  if (!contestId || !problemId || !data) return null;

  // Cooldown disabled — don't show the panel
  if (data.cooldown_seconds === 0) return null;

  const isReady = remaining === 0;

  return (
    <div
      style={{
        border: '1px solid var(--border, #e5e7eb)',
        borderRadius: 8,
        padding: 16,
        background: 'var(--card, #fff)',
      }}
    >
      <div
        style={{
          fontSize: 12,
          fontWeight: 600,
          textTransform: 'uppercase',
          letterSpacing: '0.05em',
          color: 'var(--muted-foreground, #888)',
          marginBottom: 12,
        }}
      >
        {t('cooldown.cooldown')}
      </div>

      <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
        {/* Status dot */}
        <span
          style={{
            display: 'inline-block',
            width: 8,
            height: 8,
            borderRadius: '50%',
            background: isReady ? '#10b981' : '#f59e0b',
            flexShrink: 0,
          }}
        />

        {isReady ? (
          <span style={{ fontSize: 13, color: '#10b981', fontWeight: 500 }}>
            {t('cooldown.ready')}
          </span>
        ) : (
          <span
            style={{ ...MONO, fontSize: 13, color: '#f59e0b', fontWeight: 500 }}
          >
            {t('cooldown.waitShort', { seconds: remaining })}
          </span>
        )}
      </div>

      {/* Progress bar when cooling down */}
      {!isReady && data.cooldown_seconds > 0 && (
        <div
          style={{
            marginTop: 10,
            height: 4,
            borderRadius: 2,
            background: 'var(--muted, #f3f4f6)',
            overflow: 'hidden',
          }}
        >
          <div
            style={{
              height: '100%',
              width: `${((data.cooldown_seconds - remaining) / data.cooldown_seconds) * 100}%`,
              borderRadius: 2,
              background: '#f59e0b',
              transition: 'width 1s linear',
            }}
          />
        </div>
      )}

      <div
        style={{
          marginTop: 8,
          fontSize: 11,
          color: 'var(--muted-foreground, #888)',
        }}
      >
        {t('cooldown.betweenSubmissions', { seconds: data.cooldown_seconds })}
      </div>
    </div>
  );
}
