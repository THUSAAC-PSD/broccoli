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
    'Given two integers $A$ and $B$, calculate their sum $S = A + B$.\n\nThis is a simple problem to help you get familiar with the online judge system.\n\n> **Hint:** This problem tests basic I/O operations.',
  inputFormat:
    'The first line contains two integers $A$ and $B$ $(1 \\leq A, B \\leq 10^9)$ separated by a space.',
  outputFormat:
    'Output a single integer $S$ representing the sum of $A$ and $B$.\n\nThe answer is guaranteed to fit in a **64-bit signed integer**.',
  examples: [
    {
      input: '1 2',
      output: '3',
      explanation: '$1 + 2 = 3$',
    },
    {
      input: '100 200',
      output: '300',
    },
  ],
  notes:
    'Be careful with **integer overflow**. Since $A, B \\leq 10^9$, their sum can be up to $2 \\times 10^9$, which exceeds the range of a 32-bit integer.\n\nUse `long long` in C++ or `int64` in Go:\n\n```cpp\nlong long a, b;\ncin >> a >> b;\ncout << a + b << endl;\n```',
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
