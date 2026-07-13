import type { components } from '@/api/schema';

export type SubmissionStatus = components['schemas']['SubmissionStatus'];

export type Submission = components['schemas']['SubmissionResponse'];
export type SubmissionSummary = components['schemas']['SubmissionListItem'];
export type SubmissionJudgement =
  components['schemas']['SubmissionJudgementResponse'];

export type JudgeResult = components['schemas']['JudgeResultResponse'];
export type TestCaseResult = components['schemas']['TestCaseResultResponse'];
export type Verdict = TestCaseResult['verdict'];

export type CodeRun = components['schemas']['CodeRunResponse'];
export type CodeRunJudgeResult = components['schemas']['CodeRunJudgeResult'];
export type CodeRunResult = components['schemas']['CodeRunResultResponse'];

export const SUBMISSION_STATUSES: SubmissionStatus[] = [
  'Pending',
  'Compiling',
  'Running',
  'Judged',
  'CompilationError',
  'SystemError',
];

export type SubmissionStatusFilterValue = 'all' | SubmissionStatus;

/**
 * Error envelope returned by the server for failed submissions and code
 * runs. The `{ code, message, details? }` shape is an API contract shared
 * by all submission endpoints.
 */
export interface SubmissionError {
  code: string;
  message: string;
  details?: Record<string, unknown>;
}

export const SUBMISSION_STATUS_FILTER_OPTIONS: SubmissionStatusFilterValue[] = [
  'all',
  ...SUBMISSION_STATUSES,
];
