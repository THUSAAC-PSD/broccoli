import type { ApiClient } from '@broccoli/web-sdk/api';

export interface AdditionalFile {
  id: string;
  language: string;
  path: string;
  filename: string;
  content_type: string | null;
  size: number;
  content_hash: string;
  created_at: string;
}

export const additionalFilesQueryKey = (problemId: number) => [
  'additional-files',
  problemId,
];

export function groupFilesByLanguage(
  files: AdditionalFile[],
): Record<string, AdditionalFile[]> {
  const groups: Record<string, AdditionalFile[]> = {};
  for (const file of files) {
    const lang = file.language;
    if (!groups[lang]) groups[lang] = [];
    groups[lang].push(file);
  }
  return groups;
}

export async function fetchAdditionalFiles(
  apiClient: ApiClient,
  problemId: number,
): Promise<AdditionalFile[]> {
  const { data, error } = await apiClient.GET(
    '/problems/{id}/additional-files',
    {
      params: { path: { id: problemId } },
    },
  );
  if (error) throw error;

  return (data.files ?? []).map((f) => ({
    ...f,
    content_type: f.content_type ?? null,
  }));
}
