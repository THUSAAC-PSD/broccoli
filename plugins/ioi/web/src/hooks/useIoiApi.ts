import type {
  ContestInfoResponse,
  ScoreboardResponse,
  SubmissionStatusResponse,
  SubtaskScoresResponse,
  TaskConfigResponse,
  TokenStatusResponse,
  UseTokenResponse,
} from '../types';

// Derive backend origin from where this module was loaded (the backend server),
// so API calls go to the correct host even when the page origin differs (Vite dev).
const BACKEND_ORIGIN = new URL(import.meta.url).origin;
const PLUGIN_BASE = `${BACKEND_ORIGIN}/api/v1/p/ioi/api/plugins/ioi`;
const AUTH_TOKEN_KEY = 'broccoli_token';

function authHeaders(): HeadersInit {
  const token = localStorage.getItem(AUTH_TOKEN_KEY);
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, {
    ...init,
    headers: { ...authHeaders(), ...init?.headers },
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(
      body.error || body.message || `Request failed: ${res.status}`,
    );
  }
  return res.json();
}

export function useIoiApi() {
  return {
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
  };
}
