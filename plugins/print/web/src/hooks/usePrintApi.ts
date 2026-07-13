import { useApiFetch } from '@broccoli/web-sdk/api';
import { useCallback, useMemo } from 'react';

import type {
  AdminJobsQuery,
  ArbitraryJobInput,
  ListResponse,
  PagedResponse,
  PrintJob,
  PrintStation,
  SubmitResult,
} from '../types';

const PLUGIN_BASE = '/api/v1/p/print/api/plugins/print';

export class ApiError extends Error {
  status: number;
  code?: string;

  constructor(message: string, status: number, code?: string) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
  }
}

export function usePrintApi() {
  const apiFetch = useApiFetch();

  const request = useCallback(
    async <T>(path: string, init?: RequestInit): Promise<T> => {
      const res = await apiFetch(path, init);
      if (!res.ok) {
        const body = await res
          .json()
          .catch(() => ({}) as Record<string, string>);
        throw new ApiError(
          body.error || body.message || `Request failed: ${res.status}`,
          res.status,
          body.code,
        );
      }
      // Some endpoints (rare) may return an empty body.
      return res.json().catch(() => ({}) as T);
    },
    [apiFetch],
  );

  const post = useCallback(
    <T>(path: string, body?: unknown): Promise<T> =>
      request<T>(path, {
        method: 'POST',
        headers: body ? { 'Content-Type': 'application/json' } : undefined,
        body: body ? JSON.stringify(body) : undefined,
      }),
    [request],
  );

  return useMemo(
    () => ({
      // Contestant
      printSubmission: (contestId: number, submissionId: number) =>
        post<SubmitResult>(
          `${PLUGIN_BASE}/contests/${contestId}/submissions/${submissionId}`,
        ),

      printArbitrary: (input: ArbitraryJobInput) =>
        post<SubmitResult>(`${PLUGIN_BASE}/jobs`, input),

      myJobs: (contestId?: number) =>
        request<ListResponse<PrintJob>>(
          `${PLUGIN_BASE}/jobs/mine${contestId ? `?contest_id=${contestId}` : ''}`,
        ),

      // Staff
      adminListJobs: (q: AdminJobsQuery) => {
        const params = new URLSearchParams();
        params.set('page', String(q.page));
        params.set('per_page', String(q.per_page));
        if (q.search) params.set('search', q.search);
        if (q.sort_by) params.set('sort_by', q.sort_by);
        if (q.sort_order) params.set('sort_order', q.sort_order);
        if (q.status) params.set('status', q.status);
        return request<PagedResponse<PrintJob>>(
          `${PLUGIN_BASE}/admin/jobs?${params.toString()}`,
        );
      },

      approveJob: (id: number) =>
        post(`${PLUGIN_BASE}/admin/jobs/${id}/approve`),
      reprintJob: (id: number) =>
        post(`${PLUGIN_BASE}/admin/jobs/${id}/reprint`),
      cancelJob: (id: number) => post(`${PLUGIN_BASE}/admin/jobs/${id}/cancel`),

      listStations: () =>
        request<ListResponse<PrintStation>>(`${PLUGIN_BASE}/admin/stations`),

      getJob: (id: number) =>
        request<{
          data: {
            id: number;
            source: string;
            language: string;
            filename: string;
            username: string;
            display_name: string | null;
          };
        }>(`${PLUGIN_BASE}/admin/jobs/${id}`),

      pinJob: (id: number, printer: string | null) =>
        post(`${PLUGIN_BASE}/admin/jobs/${id}/pin`, printer ? { printer } : {}),
    }),
    [post, request],
  );
}
