import { Slot } from '@broccoli/web-sdk/slot';
import type { Submission } from '@broccoli/web-sdk/submission';

import { CodeEditor, type EditorFile } from '@/components/CodeEditor';
import { SubmissionResult } from '@/features/submission/components/SubmissionResult';
import type { SubmissionError } from '@/features/submission/hooks/use-submission';

interface ProblemCodingTabProps {
  isCodeFullscreen: boolean;
  onToggleFullscreen: () => void;
  onSubmit: (files: EditorFile[], language: string) => void;
  storageKey: string;
  contestType: string;
  onContestTypeChange?: (value: string) => void;
  contestTypes?: string[];
  submissionFormat?: Record<string, string[]> | null;
  latestSubmission: Submission | null;
  submissions?: unknown[];
  isSubmitting: boolean;
  submitError: SubmissionError | null;
  contestId?: number;
  problemId: number;
}

export function ProblemCodingTab({
  isCodeFullscreen,
  onToggleFullscreen,
  onSubmit,
  storageKey,
  contestType,
  onContestTypeChange,
  contestTypes,
  submissionFormat,
  latestSubmission,
  submissions,
  isSubmitting,
  submitError,
  contestId,
  problemId,
}: ProblemCodingTabProps) {
  return (
    <div className="grid flex-1 grid-cols-1 gap-6 overflow-hidden p-6 lg:grid-cols-5">
      <div
        className={`flex flex-col overflow-hidden ${isCodeFullscreen ? 'lg:col-span-5' : 'lg:col-span-3'}`}
      >
        <CodeEditor
          onSubmit={onSubmit}
          onRun={onSubmit}
          isFullscreen={isCodeFullscreen}
          onToggleFullscreen={onToggleFullscreen}
          storageKey={storageKey}
          contestType={contestType}
          onContestTypeChange={onContestTypeChange}
          contestTypes={contestTypes}
          submissionFormat={submissionFormat}
        />
      </div>

      {!isCodeFullscreen && (
        <div className="flex min-h-0 flex-col gap-2 overflow-y-auto lg:col-span-2">
          <SubmissionResult
            submission={latestSubmission}
            isSubmitting={isSubmitting}
            error={submitError}
          />
          <Slot
            name="problem-detail.sidebar"
            as="div"
            className="flex flex-col gap-2"
            slotProps={{
              submission: latestSubmission,
              submissions,
              contestId,
              problemId,
            }}
          />
        </div>
      )}
    </div>
  );
}
