import { useState } from 'react';

import { CodeEditor } from '@/components/CodeEditor';
import { ProblemDescription } from '@/components/ProblemDescription';
import { ProblemHeader } from '@/components/ProblemHeader';
import { SubmissionResult } from '@/components/SubmissionResult';

const MOCK_PROBLEM = {
  id: 'A',
  title: 'A + B Problem',
  type: 'Default',
  io: 'Standard Input / Output',
  timeLimit: '1s',
  memoryLimit: '256 MB',
  description:
    'Given two integers A and B, calculate their sum.\n\nThis is a simple problem to help you get familiar with the online judge system.',
  inputFormat:
    'The first line contains two integers A and B separated by a space.\n\nConstraints:\n- 1 ≤ A, B ≤ 1000',
  outputFormat: 'Output a single integer representing the sum of A and B.',
  examples: [
    {
      input: '1 2',
      output: '3',
      explanation: '1 + 2 = 3',
    },
    {
      input: '100 200',
      output: '300',
      explanation: '100 + 200 = 300',
    },
  ],
  notes:
    'Make sure to handle input/output correctly. Read two integers and output their sum.',
};

type SubmissionStatus = {
  status: 'judging' | 'completed';
  verdict?: string;
  testCases?: Array<{
    id: number;
    status:
      | 'accepted'
      | 'wrong_answer'
      | 'time_limit'
      | 'runtime_error'
      | 'pending';
    time?: number;
    memory?: number;
    message?: string;
  }>;
  totalTime?: number;
  totalMemory?: number;
};

export function ProblemPage() {
  const [submissionResult, setSubmissionResult] =
    useState<SubmissionStatus | null>(null);
  const [isProblemFullscreen, setIsProblemFullscreen] = useState(false);
  const [isCodeFullscreen, setIsCodeFullscreen] = useState(false);

  const handleSubmit = (code: string, language: string) => {
    console.log('Submitting code:', { code, language });

    setSubmissionResult({
      status: 'judging',
    });

    setTimeout(() => {
      // TODO: implement real submission logic
      setSubmissionResult({
        status: 'completed',
        verdict: 'Accepted',
        totalTime: 15,
        totalMemory: 2.4,
        testCases: [
          { id: 1, status: 'accepted', time: 5, memory: 1.2 },
          { id: 2, status: 'accepted', time: 10, memory: 1.2 },
        ],
      });
    }, 2000);
  };

  const handleRun = (code: string, language: string) => {
    console.log('Running code:', { code, language });

    setSubmissionResult({
      status: 'judging',
    });

    setTimeout(() => {
      // TODO: use real submission result
      setSubmissionResult({
        status: 'completed',
        verdict: 'Custom Test Passed',
        totalTime: 8,
        totalMemory: 1.5,
        testCases: [
          {
            id: 1,
            status: 'accepted',
            time: 8,
            memory: 1.5,
            message: 'Custom test case passed',
          },
        ],
      });
    }, 1500);
  };

  return (
    <div className="flex flex-col h-full">
      <div className="p-6 pb-0">
        <ProblemHeader
          id={MOCK_PROBLEM.id}
          title={MOCK_PROBLEM.title}
          type={MOCK_PROBLEM.type}
          io={MOCK_PROBLEM.io}
          timeLimit={MOCK_PROBLEM.timeLimit}
          memoryLimit={MOCK_PROBLEM.memoryLimit}
        />
      </div>

      <div className="flex-1 grid grid-cols-1 lg:grid-cols-2 gap-6 p-6 overflow-hidden">
        {!isCodeFullscreen && (
          <div
            className={`flex flex-col gap-6 overflow-y-auto ${isProblemFullscreen ? 'col-span-2' : ''}`}
          >
            <ProblemDescription
              description={MOCK_PROBLEM.description}
              inputFormat={MOCK_PROBLEM.inputFormat}
              outputFormat={MOCK_PROBLEM.outputFormat}
              examples={MOCK_PROBLEM.examples}
              notes={MOCK_PROBLEM.notes}
              isFullscreen={isProblemFullscreen}
              onToggleFullscreen={() =>
                setIsProblemFullscreen(!isProblemFullscreen)
              }
            />
          </div>
        )}

        {!isProblemFullscreen && (
          <div
            className={`flex flex-col gap-6 overflow-y-auto ${isCodeFullscreen ? 'col-span-2' : ''}`}
          >
            <CodeEditor
              onSubmit={handleSubmit}
              onRun={handleRun}
              isFullscreen={isCodeFullscreen}
              onToggleFullscreen={() => setIsCodeFullscreen(!isCodeFullscreen)}
            />
            <SubmissionResult
              status={submissionResult?.status}
              verdict={submissionResult?.verdict}
              testCases={submissionResult?.testCases}
              totalTime={submissionResult?.totalTime}
              totalMemory={submissionResult?.totalMemory}
            />
          </div>
        )}
      </div>
    </div>
  );
}
