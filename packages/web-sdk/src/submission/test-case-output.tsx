import { useApiClient } from '@/api';
import { fetchTestCasePage } from '@/submission/fetch-test-case-page';
import type { TestCaseResult } from '@/submission/types';
import { TextPreview } from '@/ui/text-preview';

export function TestCaseOutput({
  submissionId,
  judgementId,
  testCase,
  field,
  label,
}: {
  submissionId: number;
  judgementId?: number | null;
  testCase: TestCaseResult;
  field: 'input' | 'expected_output' | 'stdout' | 'stderr' | 'checker_output';
  label: string;
}) {
  const api = useApiClient();
  const text = testCase[field];
  if (!text) return null;
  return (
    <TextPreview
      key={`${testCase.id}:${field}`}
      label={label}
      text={text}
      loadText={async (signal) => {
        const page = await fetchTestCasePage(
          api,
          submissionId,
          judgementId,
          { result_id: testCase.id, full_output: true },
          signal,
        );
        const result = page.test_case_results.find(
          (tc) => tc.id === testCase.id,
        );
        if (!result || result[field] == null)
          throw new Error('Testcase output is no longer available');
        return result[field];
      }}
    />
  );
}
