import type { components } from '@/api/schema';

export type Verdict = components['schemas']['Verdict'];
export type SubmissionStatus = components['schemas']['SubmissionStatus'];

export type Submission = components['schemas']['SubmissionResponse'];
export type SubmissionSummary = components['schemas']['SubmissionListItem'];

export type JudgeResult = components['schemas']['JudgeResultResponse'];
export type TestCaseResult = components['schemas']['TestCaseResultResponse'];

export const SUBMISSION_STATUSES: SubmissionStatus[] = [
  'Pending',
  'Compiling',
  'Running',
  'Judged',
  'CompilationError',
  'SystemError',
];

export type SubmissionStatusFilterValue = 'all' | SubmissionStatus;

export const SUBMISSION_STATUS_FILTER_OPTIONS: SubmissionStatusFilterValue[] = [
  'all',
  ...SUBMISSION_STATUSES,
];
