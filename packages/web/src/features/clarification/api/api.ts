import type { ApiClient } from '@broccoli/web-sdk/api';

import type {
  Clarification,
  CreateClarificationBody,
  ReplyClarificationBody,
} from './types';

export async function fetchClarifications(
  apiClient: ApiClient,
  contestId: number,
): Promise<Clarification[]> {
  const { data, error } = await apiClient.GET('/contests/{id}/clarifications', {
    params: { path: { id: contestId } },
  });
  if (error) throw error;
  return (data.data ?? []) as Clarification[];
}

export async function createClarification(
  apiClient: ApiClient,
  contestId: number,
  body: CreateClarificationBody,
): Promise<Clarification> {
  const { data, error } = await apiClient.POST(
    '/contests/{id}/clarifications',
    {
      params: { path: { id: contestId } },
      body,
    },
  );
  if (error) throw error;
  return data as Clarification;
}

export async function replyClarification(
  apiClient: ApiClient,
  contestId: number,
  clarificationId: number,
  body: ReplyClarificationBody,
): Promise<Clarification> {
  const { data, error } = await apiClient.POST(
    '/contests/{id}/clarifications/{clarification_id}/reply',
    {
      params: { path: { id: contestId, clarification_id: clarificationId } },
      body,
    },
  );
  if (error) throw error;
  return data as Clarification;
}
