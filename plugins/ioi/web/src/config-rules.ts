import type { ScoringMode, TokenMode } from './types';

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

export function getConfiguredScoringMode(formValues: unknown): ScoringMode {
  if (!formValues || typeof formValues !== 'object') {
    return 'max_submission';
  }

  const scoringMode = (formValues as Record<string, unknown>).scoring_mode;
  return scoringMode === 'best_tokened_or_last' ||
    scoringMode === 'sum_best_subtask'
    ? scoringMode
    : 'max_submission';
}
