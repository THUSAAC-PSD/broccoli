import { useTranslation } from '@broccoli/web-sdk/i18n';

import { getConfiguredTokenMode } from './config-rules';
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
  const selectedValue =
    typeof value === 'string' &&
    FEEDBACK_OPTIONS.some((option) => option.value === value)
      ? value
      : 'full';

  return (
    <div className="flex flex-col gap-2">
      <label className="text-[11px] font-semibold uppercase tracking-wider opacity-55">
        {schema.title ?? t('ioi.feedbackLevel.label')}
      </label>
      {schema.description && (
        <p className="text-xs opacity-50 m-0 leading-normal">
          {schema.description}
        </p>
      )}

      <select
        value={selectedValue}
        onChange={(event) => onChange(event.target.value)}
        className="flex h-9 w-full rounded-md border border-input bg-transparent px-3 py-1 text-sm shadow-sm transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
      >
        {FEEDBACK_OPTIONS.map((option) => (
          <option key={option.value} value={option.value}>
            {t(option.labelKey)}
          </option>
        ))}
      </select>

      <p className="text-xs m-0 text-muted-foreground leading-normal">
        {tokenEnabled
          ? t('ioi.feedbackLevel.tokenEnabledHint')
          : t('ioi.feedbackLevel.tokenDisabledHint')}
      </p>
    </div>
  );
}
