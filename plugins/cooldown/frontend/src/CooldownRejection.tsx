/**
 * Cooldown rejection wrapper for the `submission-result.rejection` slot.
 */
import { useTranslation } from '@broccoli/sdk/i18n';
import {
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
  useState,
} from 'react';

interface Props {
  error?: { code: string; message: string; details?: Record<string, unknown> };
  children?: ReactNode;
}

const RING_RADIUS = 40;
const RING_CIRCUMFERENCE = 2 * Math.PI * RING_RADIUS;

const MONO: React.CSSProperties = {
  fontVariantNumeric: 'tabular-nums',
  fontFamily:
    'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace',
};

function getCooldownSeconds(details?: Record<string, unknown>): number | null {
  if (!details) return null;
  const val = details.remaining_seconds;
  return typeof val === 'number' && val > 0 ? val : null;
}

export function CooldownRejection({ error, children }: Props) {
  if (error?.code !== 'COOLDOWN_ACTIVE') {
    return <>{children}</>;
  }

  return <CooldownCountdown details={error.details} />;
}

function CooldownCountdown({ details }: { details?: Record<string, unknown> }) {
  const { t } = useTranslation();
  const initialSeconds = getCooldownSeconds(details);
  const [remaining, setRemaining] = useState(initialSeconds ?? 0);
  const [expired, setExpired] = useState(initialSeconds === null);
  const startTimeRef = useRef(Date.now());
  const initialRef = useRef(initialSeconds ?? 0);

  const tick = useCallback(() => {
    const elapsed = (Date.now() - startTimeRef.current) / 1000;
    const left = Math.max(0, Math.ceil(initialRef.current - elapsed));
    setRemaining(left);
    if (left <= 0) setExpired(true);
  }, []);

  useEffect(() => {
    if (initialSeconds === null || initialSeconds <= 0) {
      setExpired(true);
      return;
    }
    startTimeRef.current = Date.now();
    initialRef.current = initialSeconds;
    setRemaining(initialSeconds);
    setExpired(false);

    const id = setInterval(tick, 200);
    return () => clearInterval(id);
  }, [initialSeconds, tick]);

  const progress = initialRef.current > 0 ? remaining / initialRef.current : 0;
  const dashOffset = RING_CIRCUMFERENCE * (1 - progress);

  return (
    <div
      style={{
        border: '1px solid var(--border, #e5e7eb)',
        borderRadius: 12,
        background: 'var(--card, #fff)',
        overflow: 'hidden',
        height: '100%',
      }}
    >
      {/* Header */}
      <div style={{ padding: '20px 24px 0' }}>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
          }}
        >
          <span
            style={{
              fontSize: 16,
              fontWeight: 600,
              color: 'var(--foreground, #111)',
            }}
          >
            {t('cooldown.result')}
          </span>
          <span
            style={{
              fontSize: 11,
              fontWeight: 500,
              padding: '2px 8px',
              borderRadius: 9999,
              background: 'var(--secondary, #f1f5f9)',
              color: 'var(--secondary-foreground, #475569)',
              display: 'inline-flex',
              alignItems: 'center',
              gap: 4,
            }}
          >
            {t('cooldown.cooldown')}
          </span>
        </div>
      </div>

      {/* Content */}
      <div
        style={{
          padding: '8px 24px 24px',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          gap: 20,
        }}
      >
        {/* Countdown ring */}
        <div style={{ position: 'relative' }}>
          <svg
            width="100"
            height="100"
            viewBox="0 0 100 100"
            style={{ transform: 'rotate(-90deg)' }}
          >
            <circle
              cx="50"
              cy="50"
              r={RING_RADIUS}
              fill="none"
              stroke="var(--muted, #f1f5f9)"
              strokeWidth="5"
            />
            <circle
              cx="50"
              cy="50"
              r={RING_RADIUS}
              fill="none"
              stroke={expired ? '#10b981' : '#f59e0b'}
              strokeWidth="5"
              strokeLinecap="round"
              strokeDasharray={RING_CIRCUMFERENCE}
              strokeDashoffset={expired ? 0 : dashOffset}
              style={{
                transition: 'stroke-dashoffset 0.3s linear, stroke 0.3s ease',
              }}
            />
          </svg>
          <div
            style={{
              position: 'absolute',
              inset: 0,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
            }}
          >
            {expired ? (
              <span style={{ fontSize: 28, color: '#10b981' }}>&#10003;</span>
            ) : (
              <span
                style={{
                  ...MONO,
                  fontSize: 24,
                  fontWeight: 700,
                  color: 'var(--foreground, #111)',
                }}
              >
                {remaining}
              </span>
            )}
          </div>
        </div>

        {/* Label */}
        <div style={{ textAlign: 'center', maxWidth: 260 }}>
          {expired ? (
            <>
              <p style={{ fontSize: 13, fontWeight: 500, color: '#10b981' }}>
                {t('cooldown.readyToSubmit')}
              </p>
              <p
                style={{
                  fontSize: 11,
                  color: 'var(--muted-foreground, #888)',
                  marginTop: 4,
                }}
              >
                {t('cooldown.canSubmitNow')}
              </p>
            </>
          ) : (
            <>
              <p
                style={{
                  fontSize: 13,
                  fontWeight: 500,
                  color: 'var(--foreground, #111)',
                }}
              >
                {t('cooldown.cooldownActive')}
              </p>
              <p
                style={{
                  fontSize: 11,
                  color: 'var(--muted-foreground, #888)',
                  marginTop: 4,
                }}
              >
                {t('cooldown.waitSeconds', { seconds: remaining })}
              </p>
            </>
          )}
        </div>

        {/* Progress bar */}
        {!expired && initialRef.current > 0 && (
          <div style={{ width: '100%', maxWidth: 200 }}>
            <div
              style={{
                height: 4,
                borderRadius: 2,
                background: 'var(--muted, #f3f4f6)',
                overflow: 'hidden',
              }}
            >
              <div
                style={{
                  height: '100%',
                  borderRadius: 2,
                  background: '#f59e0b',
                  width: `${(1 - progress) * 100}%`,
                  transition: 'width 0.3s linear',
                }}
              />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
