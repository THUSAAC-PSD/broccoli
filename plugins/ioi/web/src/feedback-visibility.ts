import type { ContestInfoResponse, TaskConfigResponse } from './types';

type EffectiveFeedback = 'none' | 'total_only' | 'subtask_scores' | 'full';

interface ResolveFeedbackVisibilityInput {
  taskConfig: Pick<TaskConfigResponse, 'feedback_level' | 'scoring_mode'>;
  contestInfo?: Pick<ContestInfoResponse, 'token_mode'> | null;
  isTokened: boolean;
  canViewPrivilegedSubmissionFeedback: boolean;
}

export interface FeedbackVisibility {
  effectiveFeedback: EffectiveFeedback;
  usesTokenMode: boolean;
  needsTokenStatus: boolean;
}

export function resolveFeedbackVisibility({
  taskConfig,
  contestInfo,
  isTokened,
  canViewPrivilegedSubmissionFeedback,
}: ResolveFeedbackVisibilityInput): FeedbackVisibility {
  const feedbackLevel = taskConfig.feedback_level;
  const usesTokenMode =
    contestInfo?.token_mode !== 'none' ||
    taskConfig.scoring_mode === 'best_tokened_or_last' ||
    feedbackLevel === 'tokened_full';

  if (canViewPrivilegedSubmissionFeedback) {
    return {
      effectiveFeedback:
        feedbackLevel === 'tokened_full'
          ? 'full'
          : normalizeFeedback(feedbackLevel),
      usesTokenMode,
      needsTokenStatus: false,
    };
  }

  if (feedbackLevel === 'tokened_full') {
    return {
      effectiveFeedback: isTokened ? 'full' : 'none',
      usesTokenMode,
      needsTokenStatus: usesTokenMode,
    };
  }

  if (
    taskConfig.scoring_mode === 'best_tokened_or_last' &&
    feedbackLevel === 'full'
  ) {
    return {
      effectiveFeedback: isTokened ? 'full' : 'subtask_scores',
      usesTokenMode,
      needsTokenStatus: usesTokenMode,
    };
  }

  return {
    effectiveFeedback: normalizeFeedback(feedbackLevel),
    usesTokenMode,
    needsTokenStatus: false,
  };
}

function normalizeFeedback(
  feedbackLevel: TaskConfigResponse['feedback_level'],
): EffectiveFeedback {
  switch (feedbackLevel) {
    case 'none':
    case 'total_only':
    case 'subtask_scores':
    case 'full':
      return feedbackLevel;
    case 'tokened_full':
      return 'none';
    default:
      return 'none';
  }
}
