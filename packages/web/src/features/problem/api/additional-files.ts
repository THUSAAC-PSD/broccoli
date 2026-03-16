import type { ApiClient } from '@broccoli/web-sdk/api';

export interface AdditionalFile {
  id: string;
  path: string;
  filename: string;
  content_type: string | null;
  size: number;
  content_hash: string;
  created_at: string;
  language: string;
  relativePath: string;
}

export const additionalFilesQueryKey = (problemId: number) => [
  'additional-files',
  problemId,
];

/** "additional_files/cpp/include/grader.h" → "cpp" */
export function extractLanguageFromPath(path: string): string {
  const segments = path.split('/');
  return segments[1] ?? '';
}

/** "additional_files/cpp/include/grader.h" → "include/grader.h" */
export function extractRelativePath(path: string): string {
  const segments = path.split('/');
  // Remove "additional_files" and language segment
  return segments.slice(2).join('/');
}

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

  return (data.attachments ?? []).map((att) => ({
    ...att,
    content_type: att.content_type ?? null,
    language: extractLanguageFromPath(att.path),
    relativePath: extractRelativePath(att.path),
  }));
}
