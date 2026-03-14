import { useApiFetch } from '@broccoli/web-sdk/api';
import { useCallback, useMemo } from 'react';

import type {
  ContestInfoResponse,
  ScoreboardResponse,
  SubmissionStatusResponse,
  SubtaskScoresResponse,
  TaskConfigResponse,
  TokenStatusResponse,
  UseTokenResponse,
} from '../types';

const PLUGIN_BASE = '/api/v1/p/ioi/api/plugins/ioi';

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

export function useIoiApi() {
  const apiFetch = useApiFetch();

  const fetchJson = useCallback(
    async <T>(path: string, init?: RequestInit): Promise<T> => {
      const res = await apiFetch(path, init);
      if (!res.ok) {
        const body = await res.json().catch(() => ({}));
        throw new ApiError(
          body.error || body.message || `Request failed: ${res.status}`,
          res.status,
          body.code,
        );
      }
      return res.json();
    },
    [apiFetch],
  );

  return useMemo(
    () => ({
      getContestInfo: (contestId: number) =>
        fetchJson<ContestInfoResponse>(
          `${PLUGIN_BASE}/contests/${contestId}/info`,
        ),

      getScoreboard: (contestId: number) =>
        fetchJson<ScoreboardResponse>(
          `${PLUGIN_BASE}/contests/${contestId}/scoreboard`,
        ),

      getTaskConfig: (contestId: number, problemId: number) =>
        fetchJson<TaskConfigResponse>(
          `${PLUGIN_BASE}/contests/${contestId}/problems/${problemId}/config`,
        ),

      getTokenStatus: (contestId: number) =>
        fetchJson<TokenStatusResponse>(
          `${PLUGIN_BASE}/contests/${contestId}/token-status`,
        ),

      useToken: (contestId: number, submissionId: number) =>
        fetchJson<UseTokenResponse>(
          `${PLUGIN_BASE}/contests/${contestId}/submissions/${submissionId}/token`,
          { method: 'POST' },
        ),

      getSubmissionSubtaskScores: (contestId: number, submissionId: number) =>
        fetchJson<SubtaskScoresResponse>(
          `${PLUGIN_BASE}/contests/${contestId}/submissions/${submissionId}/subtask-scores`,
        ),

      getSubmissionStatus: (contestId: number, problemId: number) =>
        fetchJson<SubmissionStatusResponse>(
          `${PLUGIN_BASE}/contests/${contestId}/problems/${problemId}/submission-status`,
        ),
    }),
    [fetchJson],
  );
}
