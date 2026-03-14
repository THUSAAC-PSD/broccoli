import type { components } from '@/api/schema';

export type Problem = components['schemas']['ProblemResponse'];
export type ProblemSummary = components['schemas']['ProblemListItem'];

export type TestCase = components['schemas']['TestCaseResponse'];
export type TestCaseSummary = components['schemas']['TestCaseListItem'];

export const TEST_CASE_UPLOAD_MERGE_STRATEGIES = [
  'abort',
  'skip',
  'overwrite',
  'replace',
] as const;

export type TestCaseUploadMergeStrategy =
  (typeof TEST_CASE_UPLOAD_MERGE_STRATEGIES)[number];
