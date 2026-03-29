import type { Submission } from '@broccoli/web-sdk/submission';
import { createContext, use } from 'react';

import type { EditorFile } from '@/components/CodeEditor';
import type {
  SubmissionEntry,
  UseSubmissionsReturn,
} from '@/features/submission/hooks/use-submissions';

export type { SubmissionEntry };

type SampleContentMap = Record<number, { input?: string; output?: string }>;

interface Problem {
  id: number;
  title: string;
  content: string;
  problem_type: string;
  time_limit: number;
  memory_limit: number;
  default_contest_type?: string;
  submission_format?: Record<string, string[]> | null;
  samples: Array<{
    id: number;
    input_size: number;
    output_size: number;
  }>;
  attachments?: Array<{
    id: number;
    filename: string;
    file_size: number;
  }>;
}

export interface ProblemDockContextValue {
  problem: Problem | undefined;
  isLoading: boolean;
  error: Error | null;
  sampleContents: SampleContentMap;
  copiedKey: string | null;
  onCopySample: (
    tcId: number,
    sampleIndex: number,
    type: 'input' | 'output',
    anchorEl: HTMLElement,
    inlineContent?: string,
  ) => void;
  onDownloadSample: (
    tcId: number,
    sampleIndex: number,
    type: 'input' | 'output',
  ) => void;

  // Submission
  submissions: UseSubmissionsReturn;

  // Editor
  onSubmit: (files: EditorFile[], language: string) => void;
  onRun: (
    files: EditorFile[],
    language: string,
    customTestCases: { input: string; expected_output?: string | null }[],
  ) => void;
  latestRun: SubmissionEntry | null;
  storageKey: string;
  contestType: string;
  onContestTypeChange?: (type: string) => void;
  contestTypes?: string[];
  submissionFormat?: Record<string, string[]> | null;

  // Context
  contestId?: number;
  problemId: number;

  // Latest submission for slot backward compat
  latestSubmission: Submission | null;
  latestEntry: SubmissionEntry | null;
}

const ProblemDockContext = createContext<ProblemDockContextValue | null>(null);

export const ProblemDockProvider = ProblemDockContext.Provider;

export function useProblemDockContext(): ProblemDockContextValue {
  const ctx = use(ProblemDockContext);
  if (!ctx) {
    throw new Error(
      'useProblemDockContext must be used within a ProblemDockProvider',
    );
  }
  return ctx;
}
