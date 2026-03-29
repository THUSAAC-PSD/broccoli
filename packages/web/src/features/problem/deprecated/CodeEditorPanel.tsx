import { CodeEditor } from '@/components/CodeEditor';

import { useProblemDockContext } from './dock/ProblemDockContext';

export function CodeEditorPanel() {
  const {
    onSubmit,
    onRun,
    latestRun,
    storageKey,
    contestType,
    onContestTypeChange,
    contestTypes,
    submissionFormat,
  } = useProblemDockContext();

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <CodeEditor
        onSubmit={onSubmit}
        onRun={onRun}
        latestRun={latestRun}
        storageKey={storageKey}
        contestType={contestType}
        onContestTypeChange={onContestTypeChange}
        contestTypes={contestTypes}
        submissionFormat={submissionFormat}
      />
    </div>
  );
}
