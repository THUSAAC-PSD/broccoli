/**
 * @broccoli/sdk
 * Core SDK exports
 */

// Export types
export * from './types';

// Export components
export * from './components';

// API domain types
import type { components } from '@/api/schema';

export type ContestResponse = components['schemas']['ContestResponse'];
export type ContestProblemResponse =
  components['schemas']['ContestProblemResponse'];
export type ContestListItem = components['schemas']['ContestListItem'];

export type ContestProblemItem =
  components['schemas']['ContestProblemResponse'];
export type ProblemListItem = components['schemas']['ProblemListItem'];
export type ProblemResponse = components['schemas']['ProblemResponse'];

export type Verdict = components['schemas']['Verdict'];
export type SubmissionStatus = components['schemas']['SubmissionStatus'];
export type SubmissionListItem = components['schemas']['SubmissionListItem'];
export type SubmissionResponse = components['schemas']['SubmissionResponse'];
export type JudgeResultResponse =
  components['schemas']['JudgeResultResponse'];
export type TestCaseResultResponse =
  components['schemas']['TestCaseResultResponse'];

export type TestCaseListItem = components['schemas']['TestCaseListItem'];
export type TestCaseResponse = components['schemas']['TestCaseResponse'];

export type User = components['schemas']['MeResponse'];
export type LoginRequest = components['schemas']['LoginRequest'];
