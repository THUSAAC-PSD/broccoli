import type { FeedbackLevel, TokenMode } from './types';

const FEEDBACK_LEVELS: readonly FeedbackLevel[] = [
  'full',
  'subtask_scores',
  'total_only',
  'none',
  'tokened_full',
];

function isFeedbackLevel(value: unknown): value is FeedbackLevel {
  return (
    typeof value === 'string' &&
    FEEDBACK_LEVELS.includes(value as FeedbackLevel)
  );
}

export function getConfiguredTokenMode(formValues: unknown): TokenMode {
  if (!formValues || typeof formValues !== 'object') {
    return 'none';
  }

  const tokens = (formValues as Record<string, unknown>).tokens;
  if (!tokens || typeof tokens !== 'object') {
    return 'none';
  }

  const mode = (tokens as Record<string, unknown>).mode;
  return mode === 'fixed_budget' || mode === 'regenerating' ? mode : 'none';
}

export function normalizeFeedbackLevelForTokenMode(
  feedbackLevel: unknown,
  tokenMode: TokenMode,
): FeedbackLevel | undefined {
  if (!isFeedbackLevel(feedbackLevel)) {
    return undefined;
  }

  if (tokenMode === 'none') {
    return feedbackLevel === 'tokened_full' ? 'full' : feedbackLevel;
  }

  return feedbackLevel === 'full' ? 'tokened_full' : feedbackLevel;
}
