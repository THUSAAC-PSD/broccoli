import type { components } from '@/api/schema';

export type Problem = components['schemas']['ProblemResponse'];
export type ProblemSummary = components['schemas']['ProblemListItem'];

export type TestCase = components['schemas']['TestCaseResponse'];
export type TestCaseSummary = components['schemas']['TestCaseListItem'];

// UploadTestCasesMergeStrategy may not appear as a named schema (multipart enum).
export type TestCaseMergeStrategy = 'abort' | 'skip' | 'overwrite' | 'replace';

export const TEST_CASE_MERGE_STRATEGIES: TestCaseMergeStrategy[] = [
  'abort',
  'skip',
  'overwrite',
  'replace',
] as const;
