import type { ApiClient } from '@broccoli/web-sdk/api';

export interface SupportedLanguage {
  id: string;
  name: string;
  template: string;
  defaultFilename: string;
  extensions: string[];
}

export async function fetchSupportedLanguages(
  apiClient: ApiClient,
): Promise<SupportedLanguage[]> {
  const { data, error } = await apiClient.GET('/plugins/registries');
  if (error) throw error;

  return (data.languages ?? []).map((language) => ({
    id: language.id,
    name: language.name,
    defaultFilename: language.default_filename,
    template: language.template ?? '',
    extensions: language.extensions ?? [],
  }));
}
