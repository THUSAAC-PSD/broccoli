import { appConfig } from '@/config';

import type {
  Clarification,
  ClarificationListResponse,
  CreateClarificationBody,
  ReplyClarificationBody,
} from './types';

function authHeaders(): Record<string, string> {
  const token = localStorage.getItem(appConfig.api.authTokenKey);
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${appConfig.api.baseUrl}${path}`, {
    ...init,
    headers: {
      'Content-Type': 'application/json',
      ...authHeaders(),
      ...init?.headers,
    },
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(body.message || `Request failed: ${res.status}`);
  }
  return res.json();
}

export async function fetchClarifications(
  contestId: number,
): Promise<Clarification[]> {
  const resp = await apiFetch<ClarificationListResponse>(
    `/contests/${contestId}/clarifications`,
  );
  return resp.data;
}

export async function createClarification(
  contestId: number,
  body: CreateClarificationBody,
): Promise<Clarification> {
  return apiFetch<Clarification>(`/contests/${contestId}/clarifications`, {
    method: 'POST',
    body: JSON.stringify(body),
  });
}

export async function replyClarification(
  contestId: number,
  clarificationId: number,
  body: ReplyClarificationBody,
): Promise<Clarification> {
  return apiFetch<Clarification>(
    `/contests/${contestId}/clarifications/${clarificationId}/reply`,
    { method: 'POST', body: JSON.stringify(body) },
  );
}
