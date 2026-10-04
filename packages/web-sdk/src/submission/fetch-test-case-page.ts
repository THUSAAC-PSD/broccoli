import type { ApiClient } from '@/api';
import type { operations } from '@/api/schema';

export async function fetchTestCasePage(
  api: ApiClient,
  submissionId: number,
  judgementId: number | null | undefined,
  query: operations['getSubmission']['parameters']['query'],
  signal?: AbortSignal,
) {
  if (judgementId != null) {
    const { data, error } = await api.GET(
      '/submissions/{id}/judgements/{judgement_id}',
      {
        params: {
          path: { id: submissionId, judgement_id: judgementId },
          query,
        },
        signal,
      },
    );
    if (error) throw error;
    return {
      test_case_results: data.test_case_results,
      pagination: data.test_case_pagination,
    };
  }
  const { data, error } = await api.GET('/submissions/{id}', {
    params: { path: { id: submissionId }, query },
    signal,
  });
  if (error) throw error;
  return {
    test_case_results: data.result?.test_case_results ?? [],
    pagination: data.result?.test_case_pagination,
  };
}
