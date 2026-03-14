/**
 * Replaces the `scoring_mode` dropdown with a visual timeline selector.
 * Three IOI scoring eras, rendered as a horizontal timeline with selectable cards.
 */
import { useTranslation } from '@broccoli/web-sdk/i18n';

import { getConfiguredTokenMode } from './config-rules';

interface ScoringModeSelectorProps {
  value: unknown;
  schema: { title?: string; description?: string };
  onChange: (value: unknown) => void;
  formValues?: unknown;
  setFieldValue?: (path: string[], value: unknown) => void;
}

const MODES = [
  {
    key: 'best_tokened_or_last',
    titleKey: 'ioi.scoringMode.bestTokenedOrLast.title',
    eraKey: 'ioi.scoringMode.bestTokenedOrLast.era',
    formula: 'max(tokened, last)',
    descriptionKey: 'ioi.scoringMode.bestTokenedOrLast.description',
    accent: '#0ea5e9',
  },
  {
    key: 'max_submission',
    titleKey: 'ioi.scoringMode.maxSubmission.title',
    eraKey: 'ioi.scoringMode.maxSubmission.era',
    formula: 'max(s\u2081 \u2026 s\u2099)',
    descriptionKey: 'ioi.scoringMode.maxSubmission.description',
    accent: '#8b5cf6',
  },
  {
    key: 'sum_best_subtask',
    titleKey: 'ioi.scoringMode.sumBestSubtask.title',
    eraKey: 'ioi.scoringMode.sumBestSubtask.era',
    formula: '\u03A3 max(st\u1d62)',
    descriptionKey: 'ioi.scoringMode.sumBestSubtask.description',
    accent: '#10b981',
  },
] as const;

export function ScoringModeSelector({
  value,
  schema,
  onChange,
  formValues,
  setFieldValue,
}: ScoringModeSelectorProps) {
  const { t } = useTranslation();
  const selected = typeof value === 'string' ? value : '';
  const tokenMode = getConfiguredTokenMode(formValues);

  const handleSelect = (mode: string) => {
    onChange(mode);

    if (
      mode === 'best_tokened_or_last' &&
      tokenMode === 'none' &&
      setFieldValue
    ) {
      setFieldValue(['tokens', 'mode'], 'fixed_budget');
    }
  };

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '10px',
        gridColumn: 'span 2',
      }}
    >
      <label
        style={{
          fontSize: '11px',
          fontWeight: 600,
          textTransform: 'uppercase',
          letterSpacing: '0.06em',
          opacity: 0.55,
        }}
      >
        {schema.title ?? t('ioi.scoringMode.label')}
      </label>
      {schema.description && (
        <p
          style={{ fontSize: '12px', opacity: 0.5, margin: 0, lineHeight: 1.5 }}
        >
          {schema.description}
        </p>
      )}

      {/* Timeline connector */}
      <div style={{ position: 'relative' }}>
        {/* Horizontal line behind cards */}
        <div
          style={{
            position: 'absolute',
            top: '18px',
            left: '16px',
            right: '16px',
            height: '2px',
            background: 'var(--border, #e5e7eb)',
            zIndex: 0,
          }}
        />

        <div
          style={{
            display: 'grid',
            gridTemplateColumns: '1fr 1fr 1fr',
            gap: '12px',
            position: 'relative',
            zIndex: 1,
          }}
        >
          {MODES.map((mode) => {
            const isSelected = selected === mode.key;
            const accentColor = isSelected
              ? mode.accent
              : 'var(--muted-foreground, #9ca3af)';

            return (
              <button
                key={mode.key}
                type="button"
                onClick={() => handleSelect(mode.key)}
                style={{
                  display: 'flex',
                  flexDirection: 'column',
                  alignItems: 'stretch',
                  gap: '0',
                  border: 'none',
                  padding: '0',
                  textAlign: 'left',
                  cursor: 'pointer',
                  background: 'none',
                  color: 'inherit',
                }}
              >
                {/* Timeline dot */}
                <div
                  style={{
                    display: 'flex',
                    justifyContent: 'center',
                    marginBottom: '10px',
                  }}
                >
                  <div
                    style={{
                      width: isSelected ? '14px' : '10px',
                      height: isSelected ? '14px' : '10px',
                      borderRadius: '50%',
                      background: isSelected
                        ? mode.accent
                        : 'var(--background, #fff)',
                      border: `2px solid ${accentColor}`,
                      transition: 'all 0.2s cubic-bezier(0.4, 0, 0.2, 1)',
                      boxShadow: isSelected
                        ? `0 0 0 3px color-mix(in srgb, ${mode.accent} 20%, transparent)`
                        : 'none',
                    }}
                  />
                </div>

                {/* Era label */}
                <div
                  style={{
                    fontSize: '10px',
                    fontWeight: 600,
                    letterSpacing: '0.04em',
                    color: accentColor,
                    textAlign: 'center',
                    marginBottom: '8px',
                    fontVariantNumeric: 'tabular-nums',
                  }}
                >
                  {t(mode.eraKey)}
                </div>

                {/* Card body */}
                <div
                  style={{
                    borderRadius: '8px',
                    border: isSelected
                      ? `1.5px solid ${mode.accent}`
                      : '1px solid var(--border, #e5e7eb)',
                    padding: '12px',
                    background: isSelected
                      ? `color-mix(in srgb, ${mode.accent} 4%, var(--card, #fff))`
                      : 'var(--card, #fff)',
                    transition: 'all 0.2s cubic-bezier(0.4, 0, 0.2, 1)',
                    flex: 1,
                  }}
                >
                  {/* Formula badge */}
                  <div
                    style={{
                      fontSize: '11px',
                      fontFamily:
                        'ui-monospace, "Cascadia Code", "Fira Code", Menlo, monospace',
                      fontWeight: 500,
                      padding: '3px 8px',
                      borderRadius: '4px',
                      background: isSelected
                        ? `color-mix(in srgb, ${mode.accent} 12%, transparent)`
                        : 'var(--muted, #f3f4f6)',
                      color: isSelected
                        ? mode.accent
                        : 'var(--muted-foreground, #6b7280)',
                      display: 'inline-block',
                      marginBottom: '8px',
                      letterSpacing: '0.01em',
                      transition: 'all 0.2s',
                    }}
                  >
                    {mode.formula}
                  </div>

                  <div
                    style={{
                      fontSize: '13px',
                      fontWeight: 600,
                      marginBottom: '4px',
                      opacity: isSelected ? 1 : 0.8,
                    }}
                  >
                    {t(mode.titleKey)}
                  </div>
                  <div
                    style={{
                      fontSize: '11px',
                      opacity: isSelected ? 0.65 : 0.45,
                      lineHeight: 1.5,
                      transition: 'opacity 0.2s',
                    }}
                  >
                    {t(mode.descriptionKey)}
                  </div>
                </div>
              </button>
            );
          })}
        </div>

        <details style={{ fontSize: 11, opacity: 0.6, marginTop: 8 }}>
          <summary style={{ cursor: 'pointer' }}>
            {t('ioi.scoringMode.howItWorks')}
          </summary>
          <p style={{ marginTop: 6, lineHeight: 1.6, margin: '6px 0 0' }}>
            {t('ioi.scoringMode.explanation')}
          </p>
        </details>
      </div>
    </div>
  );
}
