/**
 * Replaces the `tokens` object field with a cohesive token configuration panel.
 * Shows different fields based on the selected token mode, with a visual
 * token budget indicator.
 */
import { useTranslation } from '@broccoli/web-sdk/i18n';
import type React from 'react';

import {
  getConfiguredTokenMode,
  normalizeFeedbackLevelForTokenMode,
} from './config-rules';

interface TokenConfigPanelProps {
  value: unknown;
  schema: { title?: string; description?: string };
  onChange: (value: unknown) => void;
  formValues?: unknown;
  setFieldValue?: (path: string[], value: unknown) => void;
}

interface TokenValue {
  mode?: string;
  initial?: number;
  max?: number;
  regen_interval_min?: number;
}

const TOKEN_MODES = [
  {
    key: 'none',
    labelKey: 'ioi.tokenConfig.mode.none.label',
    descKey: 'ioi.tokenConfig.mode.none.desc',
  },
  {
    key: 'fixed_budget',
    labelKey: 'ioi.tokenConfig.mode.fixedBudget.label',
    descKey: 'ioi.tokenConfig.mode.fixedBudget.desc',
  },
  {
    key: 'regenerating',
    labelKey: 'ioi.tokenConfig.mode.regenerating.label',
    descKey: 'ioi.tokenConfig.mode.regenerating.desc',
  },
] as const;

const fieldLabel: React.CSSProperties = {
  fontSize: '10px',
  fontWeight: 600,
  textTransform: 'uppercase',
  letterSpacing: '0.05em',
  opacity: 0.5,
  display: 'block',
  marginBottom: '5px',
};

const fieldInput: React.CSSProperties = {
  width: '100%',
  padding: '7px 10px',
  borderRadius: '6px',
  border: '1px solid var(--border, #e5e7eb)',
  background: 'var(--input, #fff)',
  color: 'inherit',
  fontSize: '13px',
  fontVariantNumeric: 'tabular-nums',
  outline: 'none',
  boxSizing: 'border-box',
  transition: 'border-color 0.15s',
};

const fieldUnit: React.CSSProperties = {
  fontSize: '10px',
  opacity: 0.4,
  marginTop: '3px',
};

export function TokenConfigPanel({
  value,
  schema,
  onChange,
  formValues,
  setFieldValue,
}: TokenConfigPanelProps) {
  const { t } = useTranslation();
  const val: TokenValue = (
    typeof value === 'object' && value !== null ? value : {}
  ) as TokenValue;
  const mode = val.mode ?? 'none';
  const initial = val.initial ?? 2;
  const max = val.max ?? 5;

  const update = (patch: Partial<TokenValue>) => {
    const nextValue = { ...val, ...patch };
    onChange(nextValue);

    const nextTokenMode = getConfiguredTokenMode({
      ...formValues,
      tokens: nextValue,
    });
    const currentFeedbackLevel =
      formValues && typeof formValues === 'object'
        ? (formValues as Record<string, unknown>).feedback_level
        : undefined;
    const normalizedFeedbackLevel = normalizeFeedbackLevelForTokenMode(
      currentFeedbackLevel,
      nextTokenMode,
    );

    if (
      setFieldValue &&
      normalizedFeedbackLevel !== undefined &&
      normalizedFeedbackLevel !== currentFeedbackLevel
    ) {
      setFieldValue(['feedback_level'], normalizedFeedbackLevel);
    }
  };

  const isActive = mode !== 'none';

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '14px',
        gridColumn: 'span 2',
        padding: '16px',
        borderRadius: '10px',
        border: `1px solid ${isActive ? 'color-mix(in srgb, var(--primary, #4f46e5) 30%, var(--border, #e5e7eb))' : 'var(--border, #e5e7eb)'}`,
        background: isActive
          ? 'color-mix(in srgb, var(--primary, #4f46e5) 2%, var(--card, #fff))'
          : 'var(--card, #fff)',
        transition: 'all 0.25s cubic-bezier(0.4, 0, 0.2, 1)',
      }}
    >
      {/* Header */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
        }}
      >
        <div
          style={{
            fontSize: '11px',
            fontWeight: 600,
            textTransform: 'uppercase',
            letterSpacing: '0.05em',
            opacity: 0.55,
          }}
        >
          {schema.title ?? t('ioi.tokenConfig.title')}
        </div>
        {isActive && (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '6px',
              flexWrap: 'wrap',
            }}
          >
            <StatPill
              label={t('ioi.tokenConfig.initial')}
              value={String(initial)}
            />
            {mode === 'regenerating' && (
              <StatPill
                label={t('ioi.tokenConfig.maximum')}
                value={String(max)}
              />
            )}
          </div>
        )}
      </div>

      {/* Mode selector */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: '1fr 1fr 1fr',
          gap: '0',
          borderRadius: '8px',
          border: '1px solid var(--border, #e5e7eb)',
          overflow: 'hidden',
          background: 'var(--muted, #f3f4f6)',
        }}
      >
        {TOKEN_MODES.map((m, i) => {
          const isCurrent = mode === m.key;
          return (
            <button
              key={m.key}
              type="button"
              onClick={() => update({ mode: m.key })}
              style={{
                padding: '8px 4px',
                border: 'none',
                borderRight:
                  i < TOKEN_MODES.length - 1
                    ? '1px solid var(--border, #e5e7eb)'
                    : 'none',
                background: isCurrent ? 'var(--card, #fff)' : 'transparent',
                cursor: 'pointer',
                fontSize: '12px',
                fontWeight: isCurrent ? 600 : 400,
                color: 'inherit',
                opacity: isCurrent ? 1 : 0.6,
                transition: 'all 0.15s',
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                gap: '2px',
                position: 'relative',
              }}
            >
              <span>{t(m.labelKey)}</span>
              <span style={{ fontSize: '9px', opacity: 0.5 }}>
                {t(m.descKey)}
              </span>
              {isCurrent && (
                <div
                  style={{
                    position: 'absolute',
                    bottom: 0,
                    left: '20%',
                    right: '20%',
                    height: '2px',
                    borderRadius: '1px 1px 0 0',
                    background: 'var(--primary, #4f46e5)',
                  }}
                />
              )}
            </button>
          );
        })}
      </div>

      {/* None mode */}
      {mode === 'none' && (
        <p
          style={{
            fontSize: '12px',
            opacity: 0.45,
            margin: 0,
            fontStyle: 'italic',
            textAlign: 'center',
            padding: '8px 0',
          }}
        >
          {t('ioi.tokenConfig.disabledMessage')}
        </p>
      )}

      {/* Fixed budget mode */}
      {mode === 'fixed_budget' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '10px' }}>
          {/* Token budget visualization */}
          <div
            style={{
              display: 'flex',
              gap: '4px',
              padding: '10px 0',
              justifyContent: 'center',
            }}
          >
            {Array.from({ length: Math.min(initial, 20) }).map((_, i) => (
              <div
                key={i}
                style={{
                  width: '10px',
                  height: '10px',
                  borderRadius: '50%',
                  background: 'var(--primary, #4f46e5)',
                  opacity: 0.2 + (0.8 * (i + 1)) / Math.min(initial, 20),
                  transition: 'all 0.15s',
                  transitionDelay: `${i * 30}ms`,
                }}
              />
            ))}
          </div>
          <div>
            <label style={fieldLabel}>
              {t('ioi.tokenConfig.initialTokens')}
            </label>
            <input
              type="number"
              min={0}
              max={100}
              value={initial}
              onChange={(e) =>
                update({ initial: parseInt(e.target.value) || 0 })
              }
              style={fieldInput}
            />
            <div style={fieldUnit}>{t('ioi.tokenConfig.fixedDuration')}</div>
          </div>
        </div>
      )}

      {isActive && (
        <div
          style={{
            fontSize: 11,
            opacity: 0.5,
            fontStyle: 'italic',
            textAlign: 'center',
            padding: '4px 0',
          }}
        >
          {t('ioi.tokenConfig.scoringModeNote')}
        </div>
      )}

      {/* Regenerating mode */}
      {mode === 'regenerating' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          {/* Regen visualization: dots filling up */}
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '6px',
              padding: '8px 12px',
              borderRadius: '6px',
              background: 'var(--muted, #f3f4f6)',
              justifyContent: 'center',
            }}
          >
            <span
              style={{
                fontSize: '10px',
                opacity: 0.4,
                marginLeft: '4px',
                fontStyle: 'italic',
              }}
            >
              +1 / {val.regen_interval_min ?? 30}min
            </span>
          </div>

          <div
            style={{
              display: 'grid',
              gridTemplateColumns: '1fr 1fr 1fr',
              gap: '10px',
            }}
          >
            <div>
              <label style={fieldLabel}>{t('ioi.tokenConfig.initial')}</label>
              <input
                type="number"
                min={0}
                max={100}
                value={initial}
                onChange={(e) =>
                  update({ initial: parseInt(e.target.value) || 0 })
                }
                style={fieldInput}
              />
              <div style={fieldUnit}>{t('ioi.tokenConfig.startingTokens')}</div>
            </div>
            <div>
              <label style={fieldLabel}>{t('ioi.tokenConfig.maximum')}</label>
              <input
                type="number"
                min={0}
                max={100}
                value={max}
                onChange={(e) => update({ max: parseInt(e.target.value) || 0 })}
                style={fieldInput}
              />
              <div style={fieldUnit}>{t('ioi.tokenConfig.cap')}</div>
            </div>
            <div>
              <label style={fieldLabel}>{t('ioi.tokenConfig.interval')}</label>
              <input
                type="number"
                min={1}
                max={1440}
                value={val.regen_interval_min ?? 30}
                onChange={(e) =>
                  update({ regen_interval_min: parseInt(e.target.value) || 1 })
                }
                style={fieldInput}
              />
              <div style={fieldUnit}>
                {t('ioi.tokenConfig.minutesPerToken')}
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

function StatPill({
  label,
  value,
}: Readonly<{ label: string; value: string }>) {
  return (
    <div
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        gap: '6px',
        fontSize: '10px',
        fontWeight: 500,
        padding: '2px 8px',
        borderRadius: '10px',
        background:
          'color-mix(in srgb, var(--primary, #4f46e5) 10%, transparent)',
        color: 'var(--primary, #4f46e5)',
      }}
    >
      <span style={{ opacity: 0.7 }}>{label}</span>
      <span>{value}</span>
    </div>
  );
}
