import type { components } from '@/api/schema';

export type Problem = components['schemas']['ProblemResponse'];
export type ProblemSummary = components['schemas']['ProblemListItem'];

export type TestCase = components['schemas']['TestCaseResponse'];
export type TestCaseSummary = components['schemas']['TestCaseListItem'];
export type TestCaseUploadMergeStrategy =
  | 'abort'
  | 'skip'
  | 'overwrite'
  | 'replace';

export const TEST_CASE_UPLOAD_MERGE_STRATEGIES: TestCaseUploadMergeStrategy[] =
  ['abort', 'skip', 'overwrite', 'replace'];
