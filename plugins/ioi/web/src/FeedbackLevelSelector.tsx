import { useTranslation } from '@broccoli/web-sdk/i18n';
import { useEffect } from 'react';

import {
  getConfiguredTokenMode,
  normalizeFeedbackLevelForTokenMode,
} from './config-rules';
import type { FeedbackLevel } from './types';

interface FeedbackLevelSelectorProps {
  value: unknown;
  schema: { title?: string; description?: string };
  onChange: (value: unknown) => void;
  formValues?: unknown;
}

const FEEDBACK_OPTIONS: ReadonlyArray<{
  value: FeedbackLevel;
  labelKey: string;
}> = [
  { value: 'full', labelKey: 'ioi.contestInfo.feedback.full' },
  {
    value: 'subtask_scores',
    labelKey: 'ioi.contestInfo.feedback.subtaskScores',
  },
  { value: 'total_only', labelKey: 'ioi.contestInfo.feedback.totalOnly' },
  { value: 'none', labelKey: 'ioi.contestInfo.feedback.none' },
  {
    value: 'tokened_full',
    labelKey: 'ioi.contestInfo.feedback.tokenedFull',
  },
];

export function FeedbackLevelSelector({
  value,
  schema,
  onChange,
  formValues,
}: FeedbackLevelSelectorProps) {
  const { t } = useTranslation();
  const tokenMode = getConfiguredTokenMode(formValues);
  const tokenEnabled = tokenMode !== 'none';
  const normalizedValue = normalizeFeedbackLevelForTokenMode(value, tokenMode);

  useEffect(() => {
    if (normalizedValue !== undefined && normalizedValue !== value) {
      onChange(normalizedValue);
    }
  }, [normalizedValue, onChange, value]);

  const options = FEEDBACK_OPTIONS.filter((option) =>
    tokenEnabled ? option.value !== 'full' : option.value !== 'tokened_full',
  );

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: '8px',
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
        {schema.title ?? t('ioi.feedbackLevel.label')}
      </label>
      {schema.description && (
        <p
          style={{ fontSize: '12px', opacity: 0.5, margin: 0, lineHeight: 1.5 }}
        >
          {schema.description}
        </p>
      )}

      <select
        value={normalizedValue ?? ''}
        onChange={(event) => onChange(event.target.value)}
        className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {t(option.labelKey)}
          </option>
        ))}
      </select>

      <p
        style={{
          fontSize: '12px',
          margin: 0,
          color: 'var(--muted-foreground, #6b7280)',
          lineHeight: 1.5,
        }}
      >
        {tokenEnabled
          ? t('ioi.feedbackLevel.tokenEnabledHint')
          : t('ioi.feedbackLevel.tokenDisabledHint')}
      </p>
    </div>
  );
}
