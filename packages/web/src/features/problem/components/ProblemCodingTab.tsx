import { Slot } from '@broccoli/web-sdk/slot';
import type {
  Submission,
  SubmissionSummary,
} from '@broccoli/web-sdk/submission';

import { CodeEditor, type EditorFile } from '@/components/CodeEditor';
import { RecentSubmissionOverview } from '@/features/submission/components/RecentSubmissionOverview';
import type { SubmissionEntry } from '@/features/submission/hooks/use-submissions';

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
  submissionHistory: SubmissionSummary[];
  submissions?: SubmissionEntry[];
  isSubmitting: boolean;
  overviewVisibleCount?: number;
  submissionDetailLinkBuilder?: (submissionId: number) => string;
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
  submissionHistory,
  submissions,
  isSubmitting,
  overviewVisibleCount,
  submissionDetailLinkBuilder,
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
          <RecentSubmissionOverview
            entries={submissions ?? []}
            history={submissionHistory}
            isSubmitting={isSubmitting}
            visibleCount={overviewVisibleCount}
            linkBuilder={submissionDetailLinkBuilder}
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
