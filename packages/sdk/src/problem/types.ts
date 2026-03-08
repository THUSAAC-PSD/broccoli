import type { components } from '@/api/schema';

export type Problem = components['schemas']['ProblemResponse'];
export type ProblemSummary = components['schemas']['ProblemListItem'];

export type TestCase = components['schemas']['TestCaseResponse'];
export type TestCaseSummary = components['schemas']['TestCaseListItem'];
